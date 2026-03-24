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
