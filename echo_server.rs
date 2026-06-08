use std::io::{Read, Write};
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("0.0.0.0:9999").unwrap();
    println!("Listening on 0.0.0.0:9999");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                std::thread::spawn(move || {
                    let mut buf = [0; 1024];
                    loop {
                        match stream.read(&mut buf) {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                stream.write_all(&buf[0..n]).unwrap();
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}
