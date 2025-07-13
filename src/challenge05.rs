use regex::Regex;
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const TONY_ADDRESS: &str = "7YWHMfk9JZe0LM0g1ZauHuiSxhI";

pub async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    println!("Proxy server open on port 8000");

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

async fn handle_connection(client: TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let upstream = TcpStream::connect("chat.protohackers.com:16963").await?;

    let (client_reader, mut client_writer) = client.into_split();
    let (upstream_reader, mut upstream_writer) = upstream.into_split();

    let mut client_buf = BufReader::new(client_reader);
    let mut upstream_buf = BufReader::new(upstream_reader);

    let boguscoin_regex = Arc::new(Regex::new(r"(?:^|\s)(7[a-zA-Z0-9]{25,34})(?:\s|$)")?);

    let regex1 = boguscoin_regex.clone();
    let upstream_to_client =
        tokio::spawn(
            async move { process_lines(&mut upstream_buf, &mut client_writer, &regex1).await },
        );

    let regex2 = boguscoin_regex.clone();
    let client_to_upstream =
        tokio::spawn(
            async move { process_lines(&mut client_buf, &mut upstream_writer, &regex2).await },
        );

    tokio::try_join!(upstream_to_client, client_to_upstream)?;

    Ok(())
}

async fn process_lines<R, W>(
    reader: &mut R,
    writer: &mut W,
    boguscoin_regex: &Regex,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: AsyncBufReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await?;

        if n == 0 {
            if !buf.is_empty() {
                let line = String::from_utf8_lossy(&buf);
                let modified = process_line(&line, boguscoin_regex);
                writer.write_all(modified.as_bytes()).await?;
            }
            break;
        }

        let line = String::from_utf8_lossy(&buf);
        let modified = process_line(&line, boguscoin_regex);
        writer.write_all(modified.as_bytes()).await?;
    }

    Ok(())
}

fn process_line(line: &str, boguscoin_regex: &Regex) -> String {
    let modified = line
        .split_whitespace()
        .map(|word| {
            if boguscoin_regex.is_match(word) {
                TONY_ADDRESS
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    if line.ends_with('\n') {
        format!("{}\n", modified)
    } else {
        modified
    }
}
