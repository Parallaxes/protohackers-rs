use byteorder::{BigEndian, ByteOrder};
use std::collections::BTreeMap;
use std::io::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn run() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:4040").await?;
    println!("Server listening on port 4040");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);

        tokio::spawn(async move {
            let mut database = BTreeMap::new();
            let mut stream = socket;
            let mut buf = [0u8; 9];

            loop {
                match stream.read_exact(&mut buf).await {
                    Ok(_) => {
                        if let Err(e) = handle_client(&buf, &mut stream, &mut database).await {
                            eprintln!("handle_client error: {:?}", e);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        println!("Client disconnected.");
                        break;
                    }
                    Err(e) => {
                        eprintln!("Read error: {:?}", e);
                        break;
                    }
                }
            }
        });
    }
}

async fn handle_client(
    data: &[u8],
    stream: &mut TcpStream,
    db: &mut BTreeMap<i32, i32>,
) -> Result<()> {
    route_request(data, stream, db).await
}

async fn route_request(
    data: &[u8],
    stream: &mut TcpStream,
    db: &mut BTreeMap<i32, i32>,
) -> Result<()> {
    if data.len() != 9 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid message length",
        ));
    }

    println!("Request: {:?}", data);

    let message_type = data[0] as char;
    let first = BigEndian::read_i32(&data[1..5]);
    let second = BigEndian::read_i32(&data[5..9]);

    match message_type {
        'I' => {
            println!("Inserting: {}, {}", first, second);
            db.insert(first, second);
        }
        'Q' => {
            println!("Querying: {}, {}", first, second);
            let result = computer_mean(first, second, db);
            let mut response = [0u8; 4];
            BigEndian::write_i32(&mut response, result);
            stream.write_all(&response).await?;
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid command",
            ));
        }
    }

    Ok(())
}

fn computer_mean(mintime: i32, maxtime: i32, db: &BTreeMap<i32, i32>) -> i32 {
    if mintime > maxtime {
        return 0;
    }

    let mut sum: i64 = 0;
    let mut cnt: i64 = 0;

    for (_timestamp, &price) in db.range(mintime..=maxtime) {
        sum += price as i64;
        cnt += 1;
    }

    if cnt == 0 { 0 } else { (sum / cnt) as i32 }
}
