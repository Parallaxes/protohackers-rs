use std::collections::HashSet;
use std::io::{Error, ErrorKind, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast};

#[derive(Debug)]
struct Client {
    user: String,
    stream: TcpStream,
}

#[derive(Debug, Clone)]
struct Message {
    contents: String,
    sender: String,
    target_user: Option<String>, // None for normal chat, Some(user) for join/disconnect about that user
    class: Class,
}

#[derive(Debug, Clone, PartialEq)]
enum Class {
    Join,
    Disconnect,
    List,
    Chat,
}

pub async fn run() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8000").await?;
    println!("Server listening on port 8000");

    let (tx, _) = broadcast::channel::<Message>(100);
    let tx = Arc::new(tx);
    let users = Arc::new(Mutex::new(HashSet::<String>::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        let tx_clone = Arc::clone(&tx);
        let users_clone = Arc::clone(&users);
        let mut _rx = tx_clone.subscribe();

        println!("New connection from {}", addr);

        tokio::spawn({
            let tx_clone2 = Arc::clone(&tx);
            async move {
                match handle_init(stream, tx_clone).await {
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

                        // Subscribe before sending any messages
                        let mut client_rx = tx_clone2.subscribe();
                        let writer_half_spawn = Arc::clone(&writer_half);

                        // Spawn task to handle receiving messages for this client
                        tokio::spawn(async move {
                            while let Ok(msg) = client_rx.recv().await {
                                let mut contents = msg.contents.clone();
                                match msg.class {
                                    Class::List => {
                                        if msg.target_user == Some(user_clone2.clone()) {
                                            if let Some(list_prefix) =
                                                contents.strip_prefix("* Users online: ")
                                            {
                                                let filtered: Vec<_> = list_prefix
                                                    .trim()
                                                    .split(',')
                                                    .map(|s| s.trim())
                                                    .filter(|&u| u != user_clone2)
                                                    .collect();

                                                contents = format!(
                                                    "* Users online: {}\n",
                                                    filtered.join(", ")
                                                );
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
                                        if msg.sender == user_clone2 {
                                            continue;
                                        }
                                    }
                                }

                                println!("[{}] received: {}", user_clone2, contents);
                                let mut writer = writer_half_spawn.lock().await;
                                let _ = writer.write_all(contents.as_bytes()).await;
                            }
                        });

                        // Now broadcast join message AFTER receiver is ready
                        let join_msg = Message {
                            contents: format!("* [{}] connected.\n", user),
                            sender: "server".to_string(),
                            target_user: Some(user.clone()),
                            class: Class::Join,
                        };
                        println!("{}", join_msg.contents);
                        let _ = tx_clone2.send(join_msg.clone());

                        // Broadcast user list (to all)
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
                        let _ = tx_clone2.send(list_msg.clone());

                        // Read loop for client input
                        loop {
                            let mut line = String::new();
                            let bytes_read = reader.read_line(&mut line).await.unwrap_or(0);
                            if bytes_read == 0 {
                                println!("{} disconnected", user_clone);
                                let _ = tx_clone2.send(Message {
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
                                let _ = tx_clone2.send(msg);
                            }
                        }
                    }
                    Err(e) => eprintln!("Client error: {}", e),
                }
            }
        });
    }
}

async fn handle_init(stream: TcpStream, _tx: Arc<broadcast::Sender<Message>>) -> Result<Client> {
    let (reader_half, mut writer_half) = stream.into_split();
    writer_half
        .write_all(b"Welcome to Nyx! Please enter your username:\n")
        .await?;

    let mut reader = BufReader::new(reader_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Client disconnected"));
        }

        let username = line.trim();
        if username.is_empty() {
            writer_half
                .write_all(b"Username cannot be empty. Disconnecting client.")
                .await?;
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Username must contain chars",
            ));
        } else if username.len() > 32 {
            writer_half
                .write_all(b"Username too long (max 32 chars). Disconnecting client.")
                .await?;
            return Err(Error::new(ErrorKind::InvalidInput, "Username is too long"));
        } else if !username.chars().all(char::is_alphanumeric) {
            writer_half
                .write_all(b"Username cannot contain prohibited chars. Disconnecting client.")
                .await?;
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Username contains prohibited chars",
            ));
        } else {
            let stream = writer_half
                .reunite(reader.into_inner())
                .map_err(|_| Error::new(ErrorKind::Other, "Failed to reunite stream"))?;
            return Ok(Client {
                user: username.to_string(),
                stream,
            });
        }
    }
}
