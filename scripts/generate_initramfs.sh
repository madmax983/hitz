#!/bin/bash
set -ex

# 1. Compile a minimal static Rust binary for the TCP echo server + init
mkdir -p initramfs_workspace/src
cat << 'RUSTEOF' > initramfs_workspace/Cargo.toml
[workspace]

[package]
name = "init"
version = "0.1.0"
edition = "2021"

[dependencies]
RUSTEOF

cat << 'RUSTEOF' > initramfs_workspace/src/main.rs
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::thread;

fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                if let Err(e) = stream.write_all(&buffer[0..n]) {
                    eprintln!("Failed to write to stream: {}", e);
                    break;
                }
            }
            Err(e) => {
                eprintln!("Failed to read from stream: {}", e);
                break;
            }
        }
    }
}

fn main() {
    println!("Minimal Rust Init started.");

    // Mount virtual filesystems
    Command::new("busybox").args(&["mount", "-t", "proc", "none", "/proc"]).status().ok();
    Command::new("busybox").args(&["mount", "-t", "sysfs", "none", "/sys"]).status().ok();
    Command::new("busybox").args(&["mount", "-t", "devtmpfs", "none", "/dev"]).status().ok();

    // Bring up loopback
    Command::new("busybox").args(&["ifconfig", "lo", "up"]).status().ok();

    // Bring up eth0 (virtio-net)
    Command::new("busybox").args(&["ifconfig", "eth0", "up"]).status().ok();
    Command::new("busybox").args(&["ifconfig", "eth0", "192.168.100.2", "netmask", "255.255.255.0"]).status().ok();

    println!("Network configured.");

    // Start TCP listener
    thread::spawn(|| {
        println!("Starting TCP listener on 0.0.0.0:9999...");
        let listener = TcpListener::bind("0.0.0.0:9999").expect("Failed to bind to port 9999");
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("New connection: {}", stream.peer_addr().unwrap());
                    thread::spawn(move || handle_client(stream));
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    });

    // Fallback to a shell or wait forever
    println!("Dropping to shell or sleeping...");
    if let Ok(mut child) = Command::new("busybox").arg("sh").spawn() {
        child.wait().ok();
    } else {
        loop { thread::park(); }
    }
}
RUSTEOF

cd initramfs_workspace
cargo build --release --target x86_64-unknown-linux-musl
cd ..

# 2. Build the initramfs directory structure
rm -rf initramfs-root
mkdir -p initramfs-root/{bin,sbin,etc,proc,sys,dev,usr/bin,usr/sbin}

# Copy the static rust init binary
cp initramfs_workspace/target/x86_64-unknown-linux-musl/release/init initramfs-root/init
chmod +x initramfs-root/init

# Download a static busybox (we need it for shell and standard commands)
curl -L -o initramfs-root/bin/busybox https://busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox
chmod +x initramfs-root/bin/busybox

# Create symlinks for busybox
cd initramfs-root/bin
for prog in sh ifconfig mount cat ls; do
    ln -s busybox $prog
done
cd ../..

# 3. Create the cpio archive
cd initramfs-root
find . -print0 | cpio --null -ov --format=newc > ../test-initramfs.cpio
cd ..

echo "Created test-initramfs.cpio"
