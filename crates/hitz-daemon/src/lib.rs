//! # Abstract
//!
//! The `hitz-daemon` crate provides the background coordinator for Hitz VMs. It manages
//! VM lifecycles, exposes a local HTTP API over a named pipe (on Windows) or a Unix
//! domain socket (on Unix), and handles telemetry and port forwarding.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_daemon::VmManager;
//! use hitz_api::{VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut manager = VmManager::new();
//!
//!     let config = VmConfig {
//!         kernel_path: PathBuf::from("vmlinux"),
//!         initramfs_path: None,
//!         disk_path: None,
//!         ram_mib: 512,
//!         cpus: 1,
//!         cmdline: None,
//!         net: None,
//!         ports: vec![],
//!         guest_cid: 3,
//!         guest_agent: GuestAgentMode::Auto,
//!     };
//!

//!     println!("Created VM: {}", id);
//! }
//! ```
//! Hitz daemon — named pipe HTTP server and VM coordinator.

pub(crate) mod agent;
pub(crate) mod error;
pub(crate) mod port_forward;
pub(crate) mod router;
pub(crate) mod server;
pub(crate) mod state_store;
pub(crate) mod telemetry;
pub(crate) mod vm_manager;
pub(crate) mod vsock_server;

pub use error::DaemonError;
pub use server::run_server;
pub use telemetry::TelemetryGuard;
pub use vm_manager::VmManager;
mod tests;
