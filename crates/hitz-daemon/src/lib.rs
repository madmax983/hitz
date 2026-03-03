//! Hitz daemon — named pipe HTTP server and VM coordinator.

pub mod error;
pub mod vm_manager;

pub use error::DaemonError;
pub use vm_manager::VmManager;
