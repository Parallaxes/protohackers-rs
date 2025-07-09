use std::collections::HashMap;
use std::io::Result;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

pub async fn run() -> Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:8000").await?;
    println!("Server started on port 8000");

    let shared_socket = Arc::new(socket);
    let db = Arc::new(Mutex::new(HashMap::<Vec<u8>, Vec<u8>>::new()));
    {
        let mut db_guard = db.lock().await;
        db_guard.insert(b"version".to_vec(), b"Phlegethon V1.0".to_vec());
    }
    let mut buf = [0; 1000]; // 1000 bytes max

    loop {
        let (len, src_addr) = shared_socket.recv_from(&mut buf).await?;
        println!("Received connection from {}", src_addr);
        let received_data = buf[..len].to_vec();

        let db_clone = Arc::clone(&db);
        let socket_clone = Arc::clone(&shared_socket);

        tokio::spawn(async move {
            if let Err(e) = handle_client(received_data, src_addr, socket_clone, db_clone).await {
                eprintln!("Error handling client from {}: {}", src_addr, e);
            }
        });
    }
}

async fn handle_client(data: Vec<u8>, src_addr: std::net::SocketAddr, socket: Arc<UdpSocket>, db: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>) -> Result<()> {
    if let Some(pos) = data.iter().position(|&b| b == b'=') {
        println!("Request: INSERT from {}", src_addr);

        if &data[0..pos] == b"version" {
            println!("Ignored attempt to modify version");
            return Ok(());
        }

        let mut db_guard = db.lock().await;
        db_guard.insert(data[0..pos].to_vec(), data[pos + 1..].to_vec());

        println!(
            "Inserted: Key = {}, Value = {}",
            String::from_utf8_lossy(&data[0..pos]),
            String::from_utf8_lossy(&data[pos + 1..])
        );

        return Ok(());
    }

    println!("Request: RETRIEVE from {}", src_addr);
    let db_guard = db.lock().await;

    if let Some(value) = db_guard.get(&data) {
        let response_msg = format!(
            "{}={}",
            String::from_utf8_lossy(&data),
            String::from_utf8_lossy(value)
        );
        println!("Retrieved: {}", response_msg);
        socket.send_to(response_msg.as_bytes(), src_addr).await?;
    } else {
        println!("Key not found; no response sent");
    }

    Ok(())
}