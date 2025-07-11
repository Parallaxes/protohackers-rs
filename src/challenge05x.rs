use std::collections::HashSet;
use std::result::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::Sender;
use tokio::sync::{Mutex, broadcast};

#[derive(Debug)]
struct Client {
    user: String,
    stream: TcpStream,
}

#[derive(Clone, Debug)]
struct Message {
    contents: String,
    sender: String,
    target_user: Option<String>,
    class: Class,
}

#[derive(Clone, Debug)]
enum Class {
    Join,
    Disconnect,
    List,
    Chat,
}

pub async fn run() -> Result<(), &'static str> {
    let listener = TcpListener::bind("0.0.0.0:8000")
        .await
        .expect("Failed to start server");
    println!("Server opened on port 8000");

    let (tx, _) = broadcast::channel::<Message>(100);
    let tx = Arc::new(tx);
    let users = Arc::new(Mutex::new(HashSet::<String>::new()));

    loop {
        let (downstream, addr) = listener
            .accept()
            .await
            .expect("Failed to connect downstream");
        println!("New connection from {}", addr);
        let mut upstream = TcpStream::connect("chat.protohackers.com:16963")
            .await
            .expect("Failed to connect upstream");
        let tx_clone = Arc::clone(&tx);
        let users_clone = Arc::clone(&users);

        tokio::spawn(async move { let _ = handle_client(tx_clone, downstream, upstream, users_clone).await; });
    }

    Ok(())
}

async fn handle_init(stream: TcpStream) -> Result<Client, &'static str> {
    let (reader_half, mut writer_half) = stream.into_split();
    writer_half
        .write_all(b"Welcome to Nyx 2.0. Please enter your username:\n")
        .await
        .expect("Failed to write to stream");

    let mut reader = BufReader::new(reader_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .expect("Failed to read bytes");

        // dbg!(bytes_read);
        if bytes_read == 0 {
            return Err("Client disconnected");
        }

        let username = line.trim();
        if username.is_empty() {
            writer_half
                .write_all(b"Username cannot be empty. Disconnecting client.")
                .await
                .expect("Failed to write to stream");
            return Err("Username must contain chars.");
        } else if username.len() > 32 {
            writer_half
                .write_all(b"Username too long (max 32 chars). Disconnecting client.")
                .await
                .expect("Failed to write to stream");
            return Err("Username is too long.");
        } else if !username.chars().all(char::is_alphanumeric) {
            writer_half
                .write_all(b"Username cannot contain prohibited chars. Disconnecting client")
                .await
                .expect("Failed to write to stream");
            return Err("Username contains prohibited chars");
        } else {
            let stream = writer_half
                .reunite(reader.into_inner())
                .map_err(|_| "Failed to reunite stream")?;
            return Ok(Client {
                user: username.to_string(),
                stream,
            });
        }
    }
}

async fn handle_client(
    tx: Arc<Sender<Message>>,
    downstream: TcpStream,
    upstream: TcpStream,
    users_clone: Arc<Mutex<HashSet<String>>>,
) -> Result<(), &'static str> {
    let tx_clone = Arc::clone(&tx);

    match handle_init(downstream).await {
        Ok(Client { user, stream }) => {
            {
                let mut list = users_clone.lock().await;
                list.insert(user.clone());
            }

            let (reader_half, writer_half) = stream.into_split();
            let writer_half = Arc::new(Mutex::new(writer_half));
            let mut reader = BufReader::new(reader_half);
            let user_clone = user.clone();
            let user_clone2 = user.clone();

            let mut client_rx = tx_clone.subscribe();
            let writer_half_spawn = Arc::clone(&writer_half);

            tokio::spawn(async move {
                while let Ok(msg) = client_rx.recv().await {
                    let mut contents = msg.contents.clone();
                    match msg.class {
                        Class::List => {
                            if msg.target_user == Some(user_clone2.clone()) {
                                if let Some(list_prefix) = contents.strip_prefix("* Users online: ")
                                {
                                    let filtered: Vec<_> = list_prefix
                                        .trim()
                                        .split(',')
                                        .map(|s| s.trim())
                                        .filter(|&u| u != user_clone2)
                                        .collect();
                                    contents = format!("* Users online: {}\n", filtered.join(", "));
                                }
                            } else {
                                continue;
                            }
                        }
                        Class::Join | Class::Disconnect => {
                            if let Some(target) = &msg.target_user {
                                if target == &user_clone2 {
                                    continue;
                                }
                            }
                        }
                        Class::Chat => {
                            let new_msg = handle_msg(msg);
                            if new_msg.sender == user_clone2 {
                                continue;
                            }
                            contents = new_msg.contents
                        }
                    }

                    println!("[{}] received: {}", user_clone2, contents);
                    let mut writer = writer_half_spawn.lock().await;
                    let _ = writer.write_all(contents.as_bytes()).await;
                }
            });

            let join_msg = Message {
                contents: format!("* [{}] has connected.\n", user),
                sender: "server".to_string(),
                target_user: Some(user.clone()),
                class: Class::Join,
            };
            println!("{}", join_msg.contents);
            let _ = tx_clone.send(join_msg.clone());

            let usernames = {
                let list = users_clone.lock().await;
                list.iter().cloned().collect::<Vec<_>>().join(", ")
            };
            let list_msg = Message {
                contents: format!("* Users online: {}\n", usernames),
                sender: "server".to_string(),
                target_user: Some(user_clone.clone()),
                class: Class::List,
            };
            println!("{}", list_msg.contents);
            let _ = tx_clone.send(list_msg.clone());

            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line).await.unwrap_or(0);
                if bytes_read == 0 {
                    println!("{} disconnected", user_clone);
                    let _ = tx_clone.send(Message {
                        contents: format!("* {} disconnected.\n", user_clone),
                        sender: "server".to_string(),
                        target_user: None,
                        class: Class::Disconnect,
                    });
                    let mut list = users_clone.lock().await;
                    list.remove(&user_clone);
                    break;
                }

                let line = line.trim();
                if !line.is_empty() {
                    let msg = Message {
                        contents: format!("[{}] {}\n", user_clone, line),
                        sender: user_clone.clone(),
                        target_user: None,
                        class: Class::Chat,
                    };
                    println!("{}", msg.contents);
                    let _ = tx_clone.send(msg);
                }
            }
        }
        Err(e) => eprintln!("Client error: {}", e),
    }
    Ok(())
}

fn handle_msg(msg: Message) -> Message {
    let new_msg = msg
        .contents
        .split_whitespace()
        .map(|token| {
            if token.starts_with('7')
                && (26..=35).contains(&token.len())
                && token.chars().all(|c| c.is_ascii_alphanumeric())
            {
                "7YWHMfk9JZe0LM0g1ZauHuiSxhI"
            } else {
                token
            }
        })
        .collect::<Vec<&str>>()
        .join(" ");

    Message {
        contents: new_msg,
        ..msg
    }
}
