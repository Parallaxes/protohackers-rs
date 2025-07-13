use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use std::collections::{BTreeMap, HashSet};
use super::utils::{Message, parse, Plate, IAmCamera};


pub async fn run() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    println!("Server opened on port 8000");

    let cameras_db = Arc::new(Mutex::new(HashSet::<IAmCamera>::new()));
    let plate_db = Arc::new(Mutex::new(BTreeMap::<Plate, u32>::new()));


    loop {
        let (client, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(client).await {
                eprintln!("Connection error: {:?}", e);
            }
        });
    }
}

async fn handle_connection(client: TcpStream) -> Result<(), Box<dyn Error>> {
    let (reader, mut writer) = client.into_split();
    let mut client_buf = BufReader::new(reader);
    let mut buf = [0u8; 1024];

    loop {
        let n = client_buf.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        
        if let Some(msg) = parse(&buf) {
            match msg {
                Message::IAmCamera(_) => panic!(),
                _ => panic!(),
            }
        }
    }

    Ok(())
}

