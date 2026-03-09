//! Build script: cross-compile hitz-guest-agent for x86_64-unknown-linux-musl.
//! Falls back to assets/hitz-agent pre-built binary, then to an empty
//! placeholder if neither is available — `HITZ_AGENT_BIN` always points to a
//! valid (possibly zero-byte) file so `include_bytes!` in `agent.rs` compiles.

// Build scripts run outside the workspace lint scope and need panics on error.
#![allow(clippy::expect_used, clippy::redundant_closure_for_method_calls)]

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../hitz-guest-agent/src");
    println!("cargo:rerun-if-changed=assets/hitz-agent");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .expect("workspace root");

    let target_bin = workspace_root.join("target/x86_64-unknown-linux-musl/release/hitz-agent");

    let built = Command::new("cargo")
        .args([
            "zigbuild",
            "--release",
            "--target",
            "x86_64-unknown-linux-musl",
            "-p",
            "hitz-guest-agent",
        ])
        .current_dir(&workspace_root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if built && target_bin.exists() {
        println!("cargo:rustc-env=HITZ_AGENT_BIN={}", target_bin.display());
        return;
    }

    let asset = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/hitz-agent");
    if asset.exists() {
        println!("cargo:rustc-env=HITZ_AGENT_BIN={}", asset.display());
        return;
    }

    // Neither cross-compiled binary nor pre-built asset found.
    // Write a zero-byte placeholder so `include_bytes!(env!("HITZ_AGENT_BIN"))`
    // always compiles. `resolve_agent_bytes` in agent.rs checks for the empty
    // slice and degrades gracefully.
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo");
    let placeholder = PathBuf::from(&out_dir).join("hitz-agent.bin");
    std::fs::write(&placeholder, b"").expect("write empty agent placeholder");
    println!("cargo:rustc-env=HITZ_AGENT_BIN={}", placeholder.display());
    println!(
        "cargo:warning=hitz-guest-agent not available; GuestAgentMode::Auto degrades to \
         Disabled. Run: rustup target add x86_64-unknown-linux-musl && cargo install cargo-zigbuild"
    );
}
