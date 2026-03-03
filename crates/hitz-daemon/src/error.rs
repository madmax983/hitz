//! Daemon-specific error types.

use hitz_api::VmState;

/// Errors from daemon operations.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// VM with this ID was not found.
    #[error("VM not found: {0}")]
    NotFound(String),

    /// VM with this ID already exists.
    #[error("VM already exists: {0}")]
    AlreadyExists(String),

    /// VM is in a state that doesn't allow the requested action.
    #[error("VM \"{id}\" is {state:?}, expected {expected}")]
    InvalidState {
        /// VM identifier.
        id: String,
        /// Current state.
        state: VmState,
        /// What state was expected.
        expected: String,
    },

    /// VMM boot/run error.
    #[error("VMM error: {0}")]
    Vmm(#[from] hitz_vmm::VmError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error (lock poisoned, task panic, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}
