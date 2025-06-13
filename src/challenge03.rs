use tokio::net::{TcpListener, TcpStream};
use tokio::io::{BufReader, AsyncBufReadExt, AsyncWriteExt};
use std::collections::BTreeMap;
use tokio::sync::Mutex;
use std::sync::Arc;

pub async fn run() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:4040").await?;
    println!("Server listening on port 4040");

    let database = Arc::new(Mutex::new(BTreeMap::new()));

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);

        let db = Arc::clone(&database);

        tokio::spawn(async move {
            let mut reader = BufReader::new(socket);
            let mut line = String::new();

            loop {
                line.clear();

                let bytes_read = match reader.read_line(&mut line).await {
                    Ok(0) => {
                        println!("Connection closed by client");
                        return;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("Read error: {:?}", e);
                        return;
                    }
                };

                let trimmed = line.trim_end();
                let stream = reader.get_mut();
                if let Err(e) = handle_client(trimmed.as_bytes(), stream, Arc::clone(&db)).await {
                    eprintln!("handle_client error: {:?}", e);
                }
            }
        });
    }
}

async fn handle_client(
    data: &[u8],
    stream: &mut TcpStream,
    database: Arc<Mutex<BTreeMap<i32, i32>>>,
) -> std::io::Result<()> {
    let mut db = database.lock().await;
    route_request(data, stream, &mut db).await
}

async fn route_request(
    data: &[u8],
    stream: &mut TcpStream,
    database: &mut BTreeMap<i32, i32>,
) -> std::io::Result<()> {
    let data = String::from_utf8(from_hex(data)).expect("Failed to convert data to UTF-8");

    match data.chars().next() {
        Some('I') => {
            parse_insert(&data, database)?;
        }
        Some('Q') => {
            let result = parse_query(&data, database)?;
            let response = format!("{}\n", result);
            stream.write_all(response.as_bytes()).await?;
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

fn parse_insert(data: &str, db: &mut BTreeMap<i32, i32>) -> std::io::Result<()> {
    let timestamp = data[1..=4].parse::<i32>().expect("Could not parse timestamp");
    let price = data[5..=8].parse::<i32>().expect("Could not parse price");

    db.insert(timestamp, price);
    Ok(())
}

fn parse_query(data: &str, database: &mut BTreeMap<i32, i32>) -> std::io::Result<i32> {
    let mintime = data[1..=4].parse::<i32>().expect("Could not parse mintime");
    let maxtime = data[5..=8].parse::<i32>().expect("Could not parse maxtime");

    let mut sum = 0;
    let mut count = 0;

    for (&timestamp, &price) in database.range(mintime..=maxtime) {
        sum += price;
        count += 1;
    }

    if count == 0 {
        Ok(0) // avoid divide-by-zero
    } else {
        Ok(sum / count)
    }
}

fn from_hex(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len() / 2);
    for chunk in input.chunks(2) {
        if chunk.len() < 2 {
            panic!("Odd length hex input");
        }

        let hi = (chunk[0] as char).to_digit(16).expect("Invalid hex char") as u8;
        let lo = (chunk[1] as char).to_digit(16).expect("Invalid hex char") as u8;
        result.push((hi << 4) | lo);
    }

    result
}
