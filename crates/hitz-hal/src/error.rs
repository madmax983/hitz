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
    fn hal_error_display_formatting() {
        let err = HalError::CreatePartition("insuffient resources".into());
        assert_eq!(
            err.to_string(),
            "failed to create partition: insuffient resources"
        );

        let err = HalError::SetupPartition("invalid property".into());
        assert_eq!(
            err.to_string(),
            "failed to set partition property: invalid property"
        );

        let err = HalError::MapMemory {
            gpa: 0x1000,
            size: 4096,
            reason: "overlapping region".into(),
        };
        assert_eq!(
            err.to_string(),
            "failed to map guest memory at GPA 0x1000, size 0x1000: overlapping region"
        );

        let err = HalError::UnmapMemory {
            gpa: 0x2000,
            reason: "not mapped".into(),
        };
        assert_eq!(
            err.to_string(),
            "failed to unmap guest memory at GPA 0x2000: not mapped"
        );

        let err = HalError::CreateVcpu {
            vcpu_id: 1,
            reason: "exceeds limit".into(),
        };
        assert_eq!(err.to_string(), "failed to create vCPU 1: exceeds limit");

        let err = HalError::VcpuRun("triple fault".into());
        assert_eq!(err.to_string(), "vCPU run failed: triple fault");

        let err = HalError::VcpuCancel("already canceled".into());
        assert_eq!(err.to_string(), "failed to cancel vCPU: already canceled");

        let err = HalError::RegisterAccess("invalid register".into());
        assert_eq!(err.to_string(), "register access failed: invalid register");

        let err = HalError::InterruptRequest("queue full".into());
        assert_eq!(err.to_string(), "interrupt request failed: queue full");

        let err = HalError::GuestMem {
            gpa: 0x3000,
            reason: "out of bounds".into(),
        };
        assert_eq!(
            err.to_string(),
            "guest memory access at GPA 0x3000: out of bounds"
        );

        let err = HalError::InjectInterrupt("APIC disabled".into());
        assert_eq!(err.to_string(), "interrupt injection failed: APIC disabled");

        let err = HalError::NotAvailable("kvm not found".into());
        assert_eq!(err.to_string(), "hypervisor not available: kvm not found");

        let err = HalError::Platform {
            code: 42,
            message: "Win32 exception".into(),
        };
        assert_eq!(
            err.to_string(),
            "platform error (code 0x2a): Win32 exception"
        );
    }
}
