use crate::challenge06::db::{assign_id};

use super::utils::{IAmCamera, Message, Plate, parse, Client, ClientType};
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

pub async fn run() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    println!("Server opened on port 8000");

    // let cameras_db = Arc::new(Mutex::new(BTreeMap::<IAmCamera, u32>::new()));
    // let plate_db = Arc::new(Mutex::new(BTreeMap::<Plate, u32>::new()));
    // let id_db = Arc::new(Mutex::new(BTreeMap::<TcpStream, u32>::new()));
    let id_db = Arc::new(Mutex::new(BTreeMap::<SocketAddr, Client>::new()));

    loop {
        let (client, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        let id_db_clone = id_db.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(client, id_db_clone).await {
                eprintln!("Connection error: {:?}", e);
            }
        });
    }
}

async fn handle_connection(client: TcpStream, id_db: Arc<Mutex<BTreeMap<std::net::SocketAddr, Client>>>) -> Result<(), Box<dyn Error>> {
    let peer_addr = client.peer_addr()?;
    
    let (reader, mut writer) = client.into_split();
    let mut client_buf = BufReader::new(reader);
    let mut buf = [0u8; 1024];

    loop {
        let n = client_buf.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        if let Some(msg) = parse(&buf) {
            let id_db_clone = id_db.clone();
            match msg {
                Message::IAmCamera(_) => {
                    assign_id(peer_addr, id_db_clone, ClientType::Camera).await.expect("Failed to assign ID");
                },
                Message::IAmDispatcher(_) => {
                    assign_id(peer_addr, id_db_clone, ClientType::Dispatcher).await.expect("Failed to assign ID");
                },
                Message::Plate(_) => {
                    // TODO
                }
                _ => panic!(),
            }
        }
    }

    Ok(())
}
