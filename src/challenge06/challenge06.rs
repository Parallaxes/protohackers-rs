//! Speed Camera System server implementation.
//! 
//! Coordinates speed limit enforcement by managing cameras that report
//! plate observations and dispatchers that receive violation tickets.

use crate::challenge06::{
    client::{ClientState, ClientType}, db::PlateObservation, protocol::{parse, IAmCamera, Message, ParseError}
};

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

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
pub(crate) static CLIENT_ID_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn next_client_id() -> u32 {
    CLIENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    println!("Speed Camera Server listening on port 8000");

    // Single data structure for all client state
    let clients = Arc::new(Mutex::new(HashMap::<SocketAddr, ClientState>::new()));
    let plates = Arc::new(Mutex::new(BTreeMap::<(String, u32), PlateObservation>::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection from {}", addr);

        let clients_clone = clients.clone();
        let plates_clone = plates.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, clients_clone, plates_clone).await {
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize client state
    {
        let mut client_map = clients.lock().await;
        client_map.insert(addr, ClientState::new());
    }

    let mut buffer = [0u8; 1024];

    loop {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            break; // Client disconnected
        }

        let data = &buffer[..bytes_read];

        // Parse the message
        let message = match parse(data) {
            Ok(msg) => msg,
            Err(parse_err) => {
                send_error(&mut stream, "illegal msg").await?;
                return Err(Box::new(parse_err));
            }
        };

        // Validate against current client state
        let validation_result = {
            let client_map = clients.lock().await;
            let client_state = client_map.get(&addr).unwrap();
            validate_message(message, client_state)
        };

        match validation_result {
            Ok(valid_msg) => {
                if let Err(e) = process_message(valid_msg, addr, &clients, &plates).await {
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
    clients: &Arc<Mutex<HashMap<SocketAddr, ClientState>>>,
    plates: &Arc<Mutex<BTreeMap<(String, u32), PlateObservation>>>,
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

            println!("Camera {} registered: road {}, mile {}, limit {} mph",
                    client_id, camera_msg.road, camera_msg.mile, camera_msg.limit);
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

            println!("Dispatcher {} registered for roads: {:?}", client_id, dispatcher_msg.roads);
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
                            Some(ClientType::Camera { road, mile, limit }) => {
                                IAmCamera {
                                    road: *road,
                                    mile: *mile,
                                    limit: *limit,
                                }
                            }
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

            // Insert observation and check for speed violations
            {
                let mut plates_guard = plates.lock().await;

                // Look for previous observations of this plate on the same road BEFORE inserting
                let plate_key = &plate_obs.plate.plate;
                let mut potential_violations = Vec::new();

                // Search for other observations of this plate
                for ((stored_plate, stored_timestamp), stored_obs) in plates_guard.iter() {
                    if stored_plate == plate_key &&
                        stored_obs.camera.road == plate_obs.camera.road &&
                        stored_timestamp != &plate_obs.plate.timestamp &&
                        stored_obs.camera.mile != plate_obs.camera.mile {
                        
                        // Calculate the speed between observations
                        let time_diff = (plate_obs.plate.timestamp as i32 - *stored_timestamp as i32).abs() as u32;
                        let mile_diff = (plate_obs.camera.mile as i32 - stored_obs.camera.mile as i32).abs() as u32;

                        if time_diff > 0 {
                            // Speed = distance / time * 3600 (convert to mph)
                            let speed_mph = (mile_diff * 3600) / time_diff;

                            if speed_mph > stored_obs.camera.limit as u32 {
                                potential_violations.push((speed_mph, stored_obs.clone()));
                            }
                        }
                    }
                }

                // Insert the observation
                let key = (plate_obs.plate.plate.clone(), plate_obs.plate.timestamp);
                plates_guard.insert(key, plate_obs);

                // Process violations if any
                for (speed, _violation_obs) in potential_violations {
                    println!("Speed violation detected: {} mph (limit: {} mph)", speed, camera_info.limit);
                }
            }

            // Dispatch ticket
            println!("Plate observation: {} at timestamp {}", plate_msg.plate, plate_msg.timestamp);
        }

        Message::WantHeartbeat(heartbeat_msg) => {
            let mut client_map = clients.lock().await;
            let state = client_map.get_mut(&addr).unwrap();
            state.has_heartbeat = true;

            println!("Client {} requested heartbeat every {} deciseconds", addr, heartbeat_msg.interval);
            // TODO: Start heartbeat timer
        }

        _ => return Err("Unexpected message type".into())
    }

    Ok(())
}

pub(crate) fn validate_message(message: Message, client_state: &ClientState) -> Result<Message, ProtocolError> {
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
        },

        // All other messages are valid
        _ => Ok(message),
    }
}

async fn send_error(stream: &mut TcpStream, msg: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let error_data = format!("\x10{}{}", msg.len() as u8 as char, msg);
    stream.write_all(error_data.as_bytes()).await?;
    Ok(())
}