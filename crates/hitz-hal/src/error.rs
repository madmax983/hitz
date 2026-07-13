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
    fn error_display_create_partition() {
        let err = HalError::CreatePartition("no hypervisor".to_string());
        assert_eq!(err.to_string(), "failed to create partition: no hypervisor");
    }

    #[test]
    fn error_display_setup_partition() {
        let err = HalError::SetupPartition("bad config".to_string());
        assert_eq!(
            err.to_string(),
            "failed to set partition property: bad config"
        );
    }

    #[test]
    fn error_display_map_memory() {
        let err = HalError::MapMemory {
            gpa: 0x1000,
            size: 0x2000,
            reason: "overlap".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to map guest memory at GPA 0x1000, size 0x2000: overlap"
        );
    }

    #[test]
    fn error_display_unmap_memory() {
        let err = HalError::UnmapMemory {
            gpa: 0x3000,
            reason: "not mapped".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to unmap guest memory at GPA 0x3000: not mapped"
        );
    }

    #[test]
    fn error_display_create_vcpu() {
        let err = HalError::CreateVcpu {
            vcpu_id: 4,
            reason: "limit reached".to_string(),
        };
        assert_eq!(err.to_string(), "failed to create vCPU 4: limit reached");
    }

    #[test]
    fn error_display_vcpu_run() {
        let err = HalError::VcpuRun("crashed".to_string());
        assert_eq!(err.to_string(), "vCPU run failed: crashed");
    }

    #[test]
    fn error_display_vcpu_cancel() {
        let err = HalError::VcpuCancel("deadlock".to_string());
        assert_eq!(err.to_string(), "failed to cancel vCPU: deadlock");
    }

    #[test]
    fn error_display_register_access() {
        let err = HalError::RegisterAccess("invalid register".to_string());
        assert_eq!(err.to_string(), "register access failed: invalid register");
    }

    #[test]
    fn error_display_interrupt_request() {
        let err = HalError::InterruptRequest("vector out of bounds".to_string());
        assert_eq!(
            err.to_string(),
            "interrupt request failed: vector out of bounds"
        );
    }

    #[test]
    fn error_display_guest_mem() {
        let err = HalError::GuestMem {
            gpa: 0x4000,
            reason: "out of bounds".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "guest memory access at GPA 0x4000: out of bounds"
        );
    }

    #[test]
    fn error_display_inject_interrupt() {
        let err = HalError::InjectInterrupt("failed".to_string());
        assert_eq!(err.to_string(), "interrupt injection failed: failed");
    }

    #[test]
    fn error_display_not_available() {
        let err = HalError::NotAvailable("kvm not found".to_string());
        assert_eq!(err.to_string(), "hypervisor not available: kvm not found");
    }

    #[test]
    fn error_display_platform() {
        let err = HalError::Platform {
            code: 0xC0000022,
            message: "access denied".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "platform error (code 0xc0000022): access denied"
        );
    }
}
