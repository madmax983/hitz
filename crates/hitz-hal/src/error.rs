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
        let err = HalError::CreatePartition("foo".into());
        assert_eq!(err.to_string(), "failed to create partition: foo");

        let err = HalError::SetupPartition("bar".into());
        assert_eq!(err.to_string(), "failed to set partition property: bar");

        let err = HalError::MapMemory {
            gpa: 0x1000,
            size: 0x1000,
            reason: "baz".into(),
        };
        assert_eq!(
            err.to_string(),
            "failed to map guest memory at GPA 0x1000, size 0x1000: baz"
        );

        let err = HalError::UnmapMemory {
            gpa: 0x1000,
            reason: "qux".into(),
        };
        assert_eq!(
            err.to_string(),
            "failed to unmap guest memory at GPA 0x1000: qux"
        );

        let err = HalError::CreateVcpu {
            vcpu_id: 1,
            reason: "quux".into(),
        };
        assert_eq!(err.to_string(), "failed to create vCPU 1: quux");

        let err = HalError::VcpuRun("corge".into());
        assert_eq!(err.to_string(), "vCPU run failed: corge");

        let err = HalError::VcpuCancel("grault".into());
        assert_eq!(err.to_string(), "failed to cancel vCPU: grault");

        let err = HalError::RegisterAccess("garply".into());
        assert_eq!(err.to_string(), "register access failed: garply");

        let err = HalError::InterruptRequest("waldo".into());
        assert_eq!(err.to_string(), "interrupt request failed: waldo");

        let err = HalError::GuestMem {
            gpa: 0x2000,
            reason: "fred".into(),
        };
        assert_eq!(err.to_string(), "guest memory access at GPA 0x2000: fred");

        let err = HalError::InjectInterrupt("plugh".into());
        assert_eq!(err.to_string(), "interrupt injection failed: plugh");

        let err = HalError::NotAvailable("xyzzy".into());
        assert_eq!(err.to_string(), "hypervisor not available: xyzzy");

        let err = HalError::Platform {
            code: 0x42,
            message: "thud".into(),
        };
        assert_eq!(err.to_string(), "platform error (code 0x42): thud");
    }
    #[test]
    fn test_hal_error_debug() {
        let err = HalError::CreatePartition("foo".into());
        let _ = format!("{err:?}");
    }
    #[test]
    fn test_hal_error_debug_2() {
        let err = HalError::SetupPartition("bar".into());
        let _ = format!("{err:?}");

        let err = HalError::MapMemory {
            gpa: 0x1000,
            size: 0x1000,
            reason: "baz".into(),
        };
        let _ = format!("{err:?}");

        let err = HalError::UnmapMemory {
            gpa: 0x1000,
            reason: "qux".into(),
        };
        let _ = format!("{err:?}");

        let err = HalError::CreateVcpu {
            vcpu_id: 1,
            reason: "quux".into(),
        };
        let _ = format!("{err:?}");

        let err = HalError::VcpuRun("corge".into());
        let _ = format!("{err:?}");

        let err = HalError::VcpuCancel("grault".into());
        let _ = format!("{err:?}");

        let err = HalError::RegisterAccess("garply".into());
        let _ = format!("{err:?}");

        let err = HalError::InterruptRequest("waldo".into());
        let _ = format!("{err:?}");

        let err = HalError::GuestMem {
            gpa: 0x2000,
            reason: "fred".into(),
        };
        let _ = format!("{err:?}");

        let err = HalError::InjectInterrupt("plugh".into());
        let _ = format!("{err:?}");

        let err = HalError::NotAvailable("xyzzy".into());
        let _ = format!("{err:?}");

        let err = HalError::Platform {
            code: 0x42,
            message: "thud".into(),
        };
        let _ = format!("{err:?}");
    }
    #[test]
    fn test_hal_error_debug_more() {
        let err = HalError::Platform {
            code: 0,
            message: "a".into(),
        };
        let err2 = HalError::GuestMem {
            gpa: 0,
            reason: "a".into(),
        };
        let err3 = HalError::NotAvailable("a".into());
        let _ = format!("{err:?} {err2:?} {err3:?}");
    }
}
