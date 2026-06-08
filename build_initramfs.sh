#!/bin/bash
set -e

# Compile a simple Rust TCP echo server for musl
cat << 'RUST_CODE' > echo_server.rs
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
RUST_CODE

rustc --target x86_64-unknown-linux-musl -C opt-level=z -C link-arg=-s echo_server.rs -o init

# Create the cpio archive
echo "init" | cpio -o -H newc > tcp_echo_initramfs.cpio

mkdir -p /app/tests/assets
cp tcp_echo_initramfs.cpio /app/tests/assets/tcp_echo_initramfs.cpio

echo "Created /app/tests/assets/tcp_echo_initramfs.cpio"
