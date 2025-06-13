use tokio::net::{TcpListener, TcpStream};
use tokio::io::{BufReader, AsyncBufReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};

pub async fn run() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:4040").await?;
    println!("Server listening port 0.0.0.0:4040");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);

        tokio::spawn(async move {
            // Wrap socket in BufReader for line by line reading
            let mut reader = BufReader::new(socket);
            let mut line = String::new();

            loop {
                line.clear();

                let _bytes_read = match reader.read_line(&mut line).await {
                    Ok(0) => {
                        println!("Connection closed by client");
                        return;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("read error: {:?}", e);
                        return;
                    }
                };

                // Trim whitespace and newline
                let trimmed = line.trim_end();

                // Handle the request, pass mutable TcpStream (unwrap from BufReader)
                let stream = reader.get_mut();

                if let Err(e) = handle_request(trimmed.as_bytes(), stream).await {
                    eprintln!("Request error: {:?}", e);
                    // Malformed request -> disconnect client
                    return;
                }
            }
        });
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    method: String,
    number: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    method: String,
    prime: bool,
}

#[derive(Debug)]
enum Answer {
    Malformed,
}

async fn handle_request(data: &[u8], stream: &mut TcpStream) -> std::io::Result<()> {
    // Convert bytes to &str
    let input = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => {
            // Send malformed response with newline and return error to close connection
            stream.write_all(b"{\"answer\":\"Malformed\"}\n").await?;
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"));
        }
    };

    match parse_request(input) {
        Ok(response) => {
            // Write response followed by newline
            stream.write_all(response.as_bytes()).await?;
            stream.write_all(b"\n").await?;
            Ok(())
        }
        Err(Answer::Malformed) => {
            // Malformed response, then close connection
            stream.write_all(b"{\"answer\":\"Malformed\"}\n").await?;
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Malformed request"))
        }
    }
}

fn parse_request(input: &str) -> Result<String, Answer> {
    // Parse JSON request
    let request: Request = serde_json::from_str(input).map_err(|_| Answer::Malformed)?;

    // Check required fields: method == "isPrime", number is a number (any JSON number, floating point allowed)
    if request.method != "isPrime" {
        return Err(Answer::Malformed);
    }

    // According to spec: non-integers cannot be prime, so prime = false if fractional part != 0
    let is_prime_result = if request.number.fract() != 0.0 {
        false
    } else {
        is_prime(request.number as u64)
    };

    let response = Response {
        method: request.method,
        prime: is_prime_result,
    };

    // Serialize response JSON string
    serde_json::to_string(&response).map_err(|_| Answer::Malformed)
}

fn is_prime(n: u64) -> bool {
    if n <= 1 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let sqrt_n = (n as f64).sqrt() as u64;
    for i in (3..=sqrt_n).step_by(2) {
        if n % i == 0 {
            return false;
        }
    }
    true
}
