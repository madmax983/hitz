//! HAL error type.

/// Errors from hypervisor operations.
#[derive(Debug, thiserror::Error)]
pub enum HalError {
    /// Failed to create a partition.
    #[error("failed to create partition: {0}")]
    CreatePartition(String),

    /// Failed to set up a partition property.
    #[error("failed to set partition property: {0}")]
    SetupPartition(String),

    /// Failed to map guest memory.
    #[error("failed to map guest memory at GPA {gpa:#x}, size {size:#x}: {reason}")]
    MapMemory {
        /// Guest physical address.
        gpa: u64,
        /// Size in bytes.
        size: usize,
        /// Error detail.
        reason: String,
    },

    /// Failed to unmap guest memory.
    #[error("failed to unmap guest memory at GPA {gpa:#x}: {reason}")]
    UnmapMemory {
        /// Guest physical address.
        gpa: u64,
        /// Error detail.
        reason: String,
    },

    /// Failed to create a vCPU.
    #[error("failed to create vCPU {vcpu_id}: {reason}")]
    CreateVcpu {
        /// vCPU index.
        vcpu_id: u32,
        /// Error detail.
        reason: String,
    },

    /// vCPU run loop error.
    #[error("vCPU run failed: {0}")]
    VcpuRun(String),

    /// Failed to cancel a vCPU.
    #[error("failed to cancel vCPU: {0}")]
    VcpuCancel(String),

    /// Failed to get/set registers.
    #[error("register access failed: {0}")]
    RegisterAccess(String),

    /// Failed to request an interrupt.
    #[error("interrupt request failed: {0}")]
    InterruptRequest(String),

    /// The hypervisor platform is not available.
    #[error("hypervisor not available: {0}")]
    NotAvailable(String),

    /// Generic platform error with an OS error code.
    #[error("platform error (code {code:#x}): {message}")]
    Platform {
        /// OS-specific error code.
        code: u32,
        /// Human-readable description.
        message: String,
    },
}
