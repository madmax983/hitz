//! HAL error type.

/// Errors from hypervisor operations.
///
/// # Abstract
///
/// The central error type for all HAL operations. Encapsulates platform-specific
/// failure codes and provides context for cross-platform debugging.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::HalError;
///
/// fn setup_vm() -> Result<(), HalError> {
///     // Attempt to map memory, which might fail
///     Err(HalError::MapMemory {
///         gpa: 0x1000,
///         size: 4096,
///         reason: "Memory already mapped".to_string(),
///     })
/// }
///
/// match setup_vm() {
///     Ok(_) => println!("VM ready"),
///     Err(HalError::MapMemory { gpa, size, reason }) => {
///         println!("Failed to map {} bytes at 0x{:x}: {}", size, gpa, reason);
///     }
///     Err(e) => println!("Other error: {}", e),
/// }
/// ```
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

    /// Failed to read or write guest memory.
    #[error("guest memory access at GPA {gpa:#x}: {reason}")]
    GuestMem {
        /// Guest physical address.
        gpa: u64,
        /// Error detail.
        reason: String,
    },

    /// Failed to inject an interrupt.
    #[error("interrupt injection failed: {0}")]
    InjectInterrupt(String),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hal_error_display() {
        let err = HalError::CreatePartition("test".to_string());
        assert_eq!(err.to_string(), "failed to create partition: test");

        let err = HalError::MapMemory {
            gpa: 0x1000,
            size: 4096,
            reason: "oom".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to map guest memory at GPA 0x1000, size 0x1000: oom"
        );
    }

    #[test]
    fn test_hal_error_display_all() {
        assert_eq!(
            HalError::SetupPartition("test".to_string()).to_string(),
            "failed to set partition property: test"
        );
        assert_eq!(
            HalError::UnmapMemory {
                gpa: 0x1000,
                reason: "err".to_string()
            }
            .to_string(),
            "failed to unmap guest memory at GPA 0x1000: err"
        );
        assert_eq!(
            HalError::CreateVcpu {
                vcpu_id: 1,
                reason: "err".to_string()
            }
            .to_string(),
            "failed to create vCPU 1: err"
        );
        assert_eq!(
            HalError::VcpuRun("test".to_string()).to_string(),
            "vCPU run failed: test"
        );
        assert_eq!(
            HalError::VcpuCancel("test".to_string()).to_string(),
            "failed to cancel vCPU: test"
        );
        assert_eq!(
            HalError::RegisterAccess("test".to_string()).to_string(),
            "register access failed: test"
        );
        assert_eq!(
            HalError::InterruptRequest("test".to_string()).to_string(),
            "interrupt request failed: test"
        );
        assert_eq!(
            HalError::GuestMem {
                gpa: 0x1000,
                reason: "test".to_string()
            }
            .to_string(),
            "guest memory access at GPA 0x1000: test"
        );
        assert_eq!(
            HalError::InjectInterrupt("test".to_string()).to_string(),
            "interrupt injection failed: test"
        );
        assert_eq!(
            HalError::NotAvailable("test".to_string()).to_string(),
            "hypervisor not available: test"
        );
        assert_eq!(
            HalError::Platform {
                code: 1,
                message: "test".to_string()
            }
            .to_string(),
            "platform error (code 0x1): test"
        );
    }
}
