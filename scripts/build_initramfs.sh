#!/bin/bash
set -e

# We need a small cpio archive containing a statically linked TCP echo server running as init.
# We will use rust to compile a simple static binary.

mkdir -p /tmp/initramfs_build
cd /tmp/initramfs_build

cat << 'INNER_EOF' > main.rs
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
INNER_EOF

rustc --target x86_64-unknown-linux-musl -O main.rs -o init

# Create the cpio archive
echo "init" | cpio -o -H newc > initramfs.cpio

mkdir -p /app/tests/assets
cp initramfs.cpio /app/tests/assets/tcp_echo_initramfs.cpio

echo "Created /app/tests/assets/tcp_echo_initramfs.cpio"
