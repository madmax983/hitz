//! Hitz daemon — named pipe HTTP server and VM coordinator.

pub mod agent;
pub mod error;
pub mod port_forward;
pub mod router;
pub mod server;
pub mod telemetry;
pub mod vm_manager;
pub mod vsock_server;

pub use error::DaemonError;
pub use server::run_server;
pub use telemetry::TelemetryGuard;
pub use vm_manager::VmManager;
