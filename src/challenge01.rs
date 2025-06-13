use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;

pub fn run() -> std::io::Result<()> {
    println!("Port opened on 4040");
    let listener = TcpListener::bind("0.0.0.0:4040")?;

    for stream in listener.incoming() {
        println!("Received stream from {:?}", stream);
        handle_client(stream?);
    }

    Ok(())
}

fn handle_client(mut stream: TcpStream) {
    thread::spawn(move || {
        let mut buffer = [0; 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    println!("Connection close");
                    break;
                }
                Ok(n) => {
                    if let Err(e) = stream.write_all(&buffer[..n]) {
                        eprintln!("Write error: {:?}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {:?}", e);
                    break;
                }
            }
        }
    });
}