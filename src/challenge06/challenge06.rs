//! Speed Camera System server implementation.
//!
//! Coordinates speed limit enforcement by managing cameras that report
//! plate observations and dispatchers that receive violation tickets.

use crate::challenge06::{
    client::{ClientState, ClientType},
    db::PlateObservation,
    protocol::{IAmCamera, Message, ParseError, Ticket, parse},
};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

#[derive(Debug)]
pub enum ProtocolError {
    ParseError(ParseError),
    IllegalMessage,
    AlreadyIdentified,
    NotIdentified,
    DuplicateHeartbeatRequest,
}

impl From<ParseError> for ProtocolError {
    fn from(err: ParseError) -> Self {
        ProtocolError::ParseError(err)
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::ParseError(e) => write!(f, "Parse error: {}", e),
            ProtocolError::IllegalMessage => write!(f, "Illegal message"),
            ProtocolError::AlreadyIdentified => write!(f, "Client already identified"),
            ProtocolError::NotIdentified => write!(f, "Client not identified"),
            ProtocolError::DuplicateHeartbeatRequest => write!(f, "Duplicate heartbeat request"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Global client ID counter
pub(crate) static CLIENT_ID_COUNTER: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

fn next_client_id() -> u32 {
    CLIENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    println!("Speed Camera Server listening on port 8000");

    // Single data structure for all client state
    let clients = Arc::new(Mutex::new(HashMap::<SocketAddr, ClientState>::new()));
    let plates = Arc::new(Mutex::new(
        BTreeMap::<(String, u32), PlateObservation>::new(),
    ));
    let dispatchers = Arc::new(Mutex::new(HashMap::<u16, Vec<SocketAddr>>::new()));
    let dispatcher_streams = Arc::new(Mutex::new(HashMap::<
        SocketAddr,
        mpsc::UnboundedSender<Ticket>,
    >::new()));
    let ticket_queue = Arc::new(Mutex::new(HashMap::<u16, Vec<Ticket>>::new()));
    let issued_tickets = Arc::new(Mutex::new(HashSet::<(String, u16, u32)>::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection from {}", addr);

        let clients_clone = clients.clone();
        let plates_clone = plates.clone();
        let dispatchers_clone = dispatchers.clone();
        let dispatcher_streams_clone = dispatcher_streams.clone();
        let ticket_queue_clone = ticket_queue.clone();
        let issued_tickets_clone = issued_tickets.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(
                stream,
                addr,
                clients_clone,
                plates_clone,
                dispatchers_clone,
                dispatcher_streams_clone,
                ticket_queue_clone,
                issued_tickets_clone,
            )
            .await
            {
                eprintln!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    clients: Arc<Mutex<HashMap<SocketAddr, ClientState>>>,
    plates: Arc<Mutex<BTreeMap<(String, u32), PlateObservation>>>,
    dispatchers: Arc<Mutex<HashMap<u16, Vec<SocketAddr>>>>,
    dispatcher_streams: Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Ticket>>>>,
    ticket_queue: Arc<Mutex<HashMap<u16, Vec<Ticket>>>>,
    issued_tickets: Arc<Mutex<HashSet<(String, u16, u32)>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (heartbeat_tx, mut heartbeat_rx) = mpsc::unbounded_channel::<()>();
    let (ticket_tx, mut ticket_rx) = mpsc::unbounded_channel::<Ticket>();

    // Initialize client state
    {
        let mut client_map = clients.lock().await;
        client_map.insert(addr, ClientState::new());
    }

    let mut buffer = Vec::new(); // Dynamic buffer
    let mut temp_buffer = [0u8; 1024];

    loop {
        tokio::select! {
            // Handle incoming messages
            result = stream.read(&mut temp_buffer) => {
                let bytes_read = result?;
                if bytes_read == 0 {
                    break; // Client disconnected
                }

                buffer.extend_from_slice(&temp_buffer[..bytes_read]);

                while !buffer.is_empty() {
                    match try_parse_message(&buffer) {
                        Ok((message, consumed)) => {
                            // We got a complete message, remove it from buffer
                            buffer.drain(..consumed);

                            // Validate against current client state
                            let validation_result = {
                                let client_map = clients.lock().await;
                                let client_state = client_map.get(&addr).unwrap();
                                validate_message(message, client_state)
                            };

                            match validation_result {
                                Ok(valid_msg) => {
                                    if let Err(e) = process_message(valid_msg, addr, heartbeat_tx.clone(), ticket_tx.clone(), &clients, &plates, &dispatchers, &dispatcher_streams, &ticket_queue, &issued_tickets).await {
                                        send_error(&mut stream, "illegal msg").await?;
                                        return Err(e);
                                    }
                                }
                                Err(protocol_err) => {
                                    send_error(&mut stream, "illegal msg").await?;
                                    return Err(Box::new(protocol_err));
                                }
                            }
                        }
                        Err(ParseError::InsufficientData) => {
                            // Not enough data for a complete message, wait for more
                            break;
                        }
                        Err(_parse_err) => {
                            // Real parse error, send error and disconnect
                            send_error(&mut stream, "illegal msg").await?;
                        }
                    }
                }
            }

            // Handle heartbeat signals
            _ = heartbeat_rx.recv() => {
                if let Err(e) = stream.write_all(&[0x41]).await {
                    eprintln!("Failed to send heartbeat to {}: {}", addr, e);
                    break;
                } else {
                    println!("Heartbeat sent to client: {}", addr);
                }
            }

            // Handle outgoing tickets
            Some(ticket) = ticket_rx.recv() => {
                if let Err(e) = send_ticket_to_stream(&mut stream, ticket).await {
                    eprintln!("Failed to send ticket to {}: {}", addr, e);
                    break;
                }
            }
        }
    }

    // Cleanup on disconnect
    {
        let mut client_map = clients.lock().await;
        client_map.remove(&addr);
    }

    println!("Client {} disconnected", addr);
    Ok(())
}

pub async fn process_message(
    message: Message,
    addr: SocketAddr,
    heartbeat_tx: mpsc::UnboundedSender<()>,
    ticket_tx: mpsc::UnboundedSender<Ticket>,
    clients: &Arc<Mutex<HashMap<SocketAddr, ClientState>>>,
    plates: &Arc<Mutex<BTreeMap<(String, u32), PlateObservation>>>,
    dispatchers: &Arc<Mutex<HashMap<u16, Vec<SocketAddr>>>>,
    dispatcher_streams: &Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Ticket>>>>,
    ticket_queue: &Arc<Mutex<HashMap<u16, Vec<Ticket>>>>,
    issued_tickets: &Arc<Mutex<HashSet<(String, u16, u32)>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match message {
        Message::IAmCamera(camera_msg) => {
            let client_type = ClientType::Camera {
                road: camera_msg.road,
                mile: camera_msg.mile,
                limit: camera_msg.limit,
            };

            // Update client state with type and assign ID
            let client_id = {
                let mut client_map = clients.lock().await;
                let state = client_map.get_mut(&addr).unwrap();
                state.set_client_type(client_type);

                let new_id = next_client_id();
                state.client_id = Some(new_id);
                new_id
            };

            println!(
                "Camera {} registered: road {}, mile {}, limit {} mph",
                client_id, camera_msg.road, camera_msg.mile, camera_msg.limit
            );
        }

        Message::IAmDispatcher(dispatcher_msg) => {
            let client_type = ClientType::Dispatcher {
                roads: dispatcher_msg.roads.clone(),
            };

            // Update client state with type and assign ID
            let client_id = {
                let mut client_map = clients.lock().await;
                let state = client_map.get_mut(&addr).unwrap();
                state.set_client_type(client_type);

                let new_id = next_client_id();
                state.client_id = Some(new_id);
                new_id
            };

            // Register this dispatcher for its road and store its stream
            {
                let mut dispatchers_guard = dispatchers.lock().await;
                for road in &dispatcher_msg.roads {
                    dispatchers_guard.entry(*road).or_default().push(addr);
                }

                // Store the ticket sender for this dispatcher
                let mut dispatcher_streams_guard = dispatcher_streams.lock().await;
                dispatcher_streams_guard.insert(addr, ticket_tx.clone());
            }

            // send any queued tickets for these roads
            {
                let mut ticket_queue_guard = ticket_queue.lock().await;
                for road in &dispatcher_msg.roads {
                    if let Some(queued_tickets) = ticket_queue_guard.remove(road) {
                        for ticket in queued_tickets {
                            if ticket_tx.send(ticket).is_err() {
                                eprintln!("Failed to send queued tickets to dispatcher: {}", addr);
                            }
                        }
                    }
                }
            }

            println!(
                "Dispatcher {} registered for roads: {:?}",
                client_id, dispatcher_msg.roads
            );
        }

        Message::Plate(plate_msg) => {
            // Get camera info from the current client
            let (camera_id, camera_info) = {
                let client_guard = clients.lock().await;
                match client_guard.get(&addr) {
                    Some(client_state) => {
                        let id = match client_state.client_id {
                            Some(id) => id,
                            None => {
                                return Err("Client not fully identified".into());
                            }
                        };

                        let camera = match &client_state.client_type {
                            Some(ClientType::Camera { road, mile, limit }) => IAmCamera {
                                road: *road,
                                mile: *mile,
                                limit: *limit,
                            },
                            _ => {
                                return Err("Client is not a camera".into());
                            }
                        };

                        (id, camera)
                    }
                    None => {
                        return Err("Client state not found".into());
                    }
                }
            };

            // Create the observation
            let plate_obs = PlateObservation {
                plate: plate_msg.clone(),
                camera: camera_info.clone(), // Clone so we can use it later
                camera_id,
            };

            // Extract values we'll need after the move
            let plate_name = plate_obs.plate.plate.clone();
            let plate_timestamp = plate_obs.plate.timestamp;
            let camera_mile = plate_obs.camera.mile;
            let camera_road = plate_obs.camera.road;

            // Insert observation and check for speed violations
            {
                let mut plates_guard = plates.lock().await;

                // Look for previous observations of this plate on the same road BEFORE inserting
                let plate_key = &plate_obs.plate.plate;
                let mut potential_violations = Vec::new();

                // Search for other observations of this plate
                for ((stored_plate, stored_timestamp), stored_obs) in plates_guard.iter() {
                    if stored_plate == plate_key && stored_obs.camera.road == plate_obs.camera.road
                    {
                        // Calculate the speed between observations
                        let time_diff = (plate_obs.plate.timestamp as i32
                            - *stored_timestamp as i32)
                            .abs() as u32;
                        let mile_diff = (plate_obs.camera.mile as i32
                            - stored_obs.camera.mile as i32)
                            .abs() as u32;

                        if time_diff > 0 {
                            // Speed = distance / time * 3600 (convert to mph)
                            let speed_mph_precise = (mile_diff as f64 * 3600.0) / time_diff as f64;
                            let speed_mph = speed_mph_precise.round() as u32;

                            // Use the minimum speed limit from both cameras
                            let limit =
                                std::cmp::min(stored_obs.camera.limit, plate_obs.camera.limit);

                            // Rounding rule: ticket only if speed > limit + 0.5 mph
                            if (speed_mph * 10) > (limit as u32 * 10 + 5) {
                                potential_violations.push((speed_mph, stored_obs.clone()));
                            }
                        }
                    }
                }

                // Insert the observation
                let key = (plate_obs.plate.plate.clone(), plate_obs.plate.timestamp);
                plates_guard.insert(key, plate_obs);

                // Process violations if any
                for (speed, violation_obs) in potential_violations {
                    // Use the minimum limit from both cameras for display
                    let limit = std::cmp::min(violation_obs.camera.limit, camera_info.limit);
                    println!(
                        "Speed violation detected: {} mph (limit: {} mph)",
                        speed, limit
                    );

                    // Ensure chronological ordering
                    let (earlier_obs, later_obs) =
                        if violation_obs.plate.timestamp <= plate_timestamp {
                            // violate_obs is earlier
                            (
                                (violation_obs.camera.mile, violation_obs.plate.timestamp),
                                (camera_mile, plate_timestamp),
                            )
                        } else {
                            // current observation is earlier
                            (
                                (camera_mile, plate_timestamp),
                                (violation_obs.camera.mile, violation_obs.plate.timestamp),
                            )
                        };

                    let ticket = Ticket {
                        plate: plate_name.clone(),
                        road: camera_road,
                        mile1: earlier_obs.0,
                        timestamp1: earlier_obs.1,
                        mile2: later_obs.0,
                        timestamp2: later_obs.1,
                        speed: speed as u16 * 100, // Convert to mph * 100
                    };

                    // Calculate the day range for the ticket
                    let start_day = earlier_obs.1 / 86400;
                    let end_day = later_obs.1 / 86400;

                    // Check if the ticket was already issued
                    let mut issued_tickets_guard = issued_tickets.lock().await;
                    let mut already_issued = false;
                    for day in start_day..=end_day {
                        if issued_tickets_guard.contains(&(ticket.plate.clone(), ticket.road, day))
                        {
                            already_issued = true;
                            break;
                        }
                    }

                    if already_issued {
                        continue; // Skip duplicate tickets
                    }

                    // Mark all days in the range as issued
                    for day in start_day..=end_day {
                        issued_tickets_guard.insert((ticket.plate.clone(), ticket.road, day));
                    }

                    // Send or queue the ticket
                    let dispatcher_streams_guard = dispatcher_streams.lock().await;
                    let dispatchers_guard = dispatchers.lock().await;

                    if let Some(dispatcher_list) = dispatchers_guard.get(&ticket.road) {
                        if let Some(dispatcher_addr) = dispatcher_list.first() {
                            if let Some(ticket_sender) =
                                dispatcher_streams_guard.get(dispatcher_addr)
                            {
                                if ticket_sender.send(ticket.clone()).is_err() {
                                    eprintln!(
                                        "Failed to send ticket to dispatcher: {}",
                                        dispatcher_addr
                                    );
                                }
                                println!("Ticket sent to dispatcher: {}", dispatcher_addr);
                            }
                        }
                    } else {
                        // Queue the ticket
                        let mut ticket_queue_guard = ticket_queue.lock().await;
                        ticket_queue_guard
                            .entry(ticket.road)
                            .or_default()
                            .push(ticket);
                    }
                }
            }

            println!(
                "Plate observation: {} at timestamp {}",
                plate_msg.plate, plate_msg.timestamp
            );
        }

        Message::WantHeartbeat(heartbeat_msg) => {
            let mut client_map = clients.lock().await;
            let state = client_map.get_mut(&addr).unwrap();

            // Check if heartbeat was already requested
            if state.has_heartbeat {
                return Err("Duplicate heartbeat request".into());
            }

            // Update the client state
            state.has_heartbeat = true;
            state.heartbeat_interval = Some(heartbeat_msg.interval);

            println!(
                "Client {} requested heartbeat every {} deciseconds",
                addr, heartbeat_msg.interval
            );

            // Start the heartbeat task
            if heartbeat_msg.interval > 0 {
                let interval = heartbeat_msg.interval;
                let addr_clone = addr.clone();
                let clients_clone = clients.clone();
                let heartbeat_sender = heartbeat_tx.clone();
                tokio::spawn(async move {
                    send_heartbeat(addr_clone, interval, clients_clone, heartbeat_sender).await;
                });
            }
        }

        _ => return Err("Unexpected message type".into()),
    }

    Ok(())
}

fn try_parse_message(data: &[u8]) -> Result<(Message, usize), ParseError> {
    if data.is_empty() {
        return Err(ParseError::InsufficientData);
    }

    let message_type = data[0];

    match message_type {
        0x20 => {
            // Need at least: 1 byte for string length + string + 4 bytes for timestamp
            if data.len() < 2 {
                return Err(ParseError::InsufficientData);
            }

            let string_len = data[1] as usize;
            let required_len = 1 + 1 + string_len + 4; // type + len + string + timestamp

            if data.len() < required_len {
                return Err(ParseError::InsufficientData);
            }

            let message = parse(data)?;
            Ok((message, required_len))
        }
        0x40 => {
            let required_len = 1 + 4; // type + interval (u32)
            if data.len() < required_len {
                return Err(ParseError::InsufficientData);
            }

            let message = parse(data)?;
            Ok((message, required_len))
        }
        0x80 => {
            let required_len = 1 + 2 + 2 + 2; // type + road + mile + limit
            if data.len() < required_len {
                return Err(ParseError::InsufficientData);
            }

            let message = parse(data)?;
            Ok((message, required_len))
        }
        0x81 => {
            if data.len() < 2 {
                return Err(ParseError::InsufficientData);
            }

            let num_roads = data[1] as usize;
            let required_len = 1 + 1 + (num_roads * 2); // type + numroads + roads

            if data.len() < required_len {
                return Err(ParseError::InsufficientData);
            }

            let message = parse(data)?;
            Ok((message, required_len))
        }
        _ => {
            let message = parse(data)?;
            Ok((message, 1)) // For unknown/error messages, just consume the type byte
        }
    }
}

pub(crate) fn validate_message(
    message: Message,
    client_state: &ClientState,
) -> Result<Message, ProtocolError> {
    match (&message, &client_state.client_type) {
        // Plate messages only from cameras
        (Message::Plate(_), Some(ClientType::Camera { .. })) => Ok(message),
        (Message::Plate(_), _) => Err(ProtocolError::NotIdentified),

        // Can't identify twice
        (Message::IAmCamera(_), Some(_)) => Err(ProtocolError::AlreadyIdentified),
        (Message::IAmDispatcher(_), Some(_)) => Err(ProtocolError::AlreadyIdentified),

        // Can't request heartbeat twice
        (Message::WantHeartbeat(_), _) if client_state.has_heartbeat => {
            Err(ProtocolError::DuplicateHeartbeatRequest)
        }

        // All other messages are valid
        _ => Ok(message),
    }
}

async fn send_ticket_to_stream(
    stream: &mut TcpStream,
    ticket: Ticket,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Serialize the ticket into the binary protocol format
    let mut data = Vec::new();
    data.push(0x21); // Ticket message type
    data.push(ticket.plate.len() as u8); // String length as u8, not u32
    data.extend(ticket.plate.as_bytes());
    data.extend(ticket.road.to_be_bytes());
    data.extend(ticket.mile1.to_be_bytes());
    data.extend(ticket.timestamp1.to_be_bytes());
    data.extend(ticket.mile2.to_be_bytes());
    data.extend(ticket.timestamp2.to_be_bytes());
    data.extend(ticket.speed.to_be_bytes());

    // Send the data to the dispatcher
    stream.write_all(&data).await?;
    println!(
        "Ticket sent: {} on road {} at {} mph",
        ticket.plate,
        ticket.road,
        ticket.speed as f32 / 100.0
    );
    Ok(())
}

async fn send_heartbeat(
    addr: SocketAddr,
    interval: u32,
    clients: Arc<Mutex<HashMap<SocketAddr, ClientState>>>,
    heartbeat_tx: mpsc::UnboundedSender<()>,
) {
    println!(
        "Starting heartbeat task for client: {} with interval: {} deciseconds",
        addr, interval
    );
    let mut interval =
        tokio::time::interval(std::time::Duration::from_millis((interval * 100) as u64));
    loop {
        interval.tick().await;
        println!("Heartbeat tick for client: {}", addr);

        // Check if the client is still connected and wants heartbeat
        let should_continue = {
            println!("Checking heartbeat state for client: {}", addr);
            let client_map = clients.lock().await;
            if let Some(state) = client_map.get(&addr) {
                state.heartbeat_interval.unwrap_or(0) > 0
            } else {
                false // Client disconnected
            }
        };

        if !should_continue {
            println!(
                "Client {} no longer wants heartbeats or is disconnected",
                addr
            );
            break;
        }

        // Send the heartbeat signal through channel
        if heartbeat_tx.send(()).is_err() {
            println!("Client {} disconnected, stopping heartbeat", addr);
            break;
        }
    }
    println!("Stopping heartbeat task for client: {}", addr);
}

async fn send_error(stream: &mut TcpStream, msg: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let error_data = format!("\x10{}{}", msg.len() as u8 as char, msg);
    stream.write_all(error_data.as_bytes()).await?;
    Ok(())
}
