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
        let create_err = HalError::CreatePartition("out of memory".to_string());
        assert_eq!(
            create_err.to_string(),
            "failed to create partition: out of memory"
        );

        let setup_err = HalError::SetupPartition("invalid property".to_string());
        assert_eq!(
            setup_err.to_string(),
            "failed to set partition property: invalid property"
        );

        let map_err = HalError::MapMemory {
            gpa: 0x2000,
            size: 0x1000,
            reason: "overlap".to_string(),
        };
        assert_eq!(
            map_err.to_string(),
            "failed to map guest memory at GPA 0x2000, size 0x1000: overlap"
        );

        let unmap_err = HalError::UnmapMemory {
            gpa: 0x4000,
            reason: "not found".to_string(),
        };
        assert_eq!(
            unmap_err.to_string(),
            "failed to unmap guest memory at GPA 0x4000: not found"
        );

        let create_vcpu_err = HalError::CreateVcpu {
            vcpu_id: 2,
            reason: "limit reached".to_string(),
        };
        assert_eq!(
            create_vcpu_err.to_string(),
            "failed to create vCPU 2: limit reached"
        );

        let run_err = HalError::VcpuRun("triple fault".to_string());
        assert_eq!(run_err.to_string(), "vCPU run failed: triple fault");

        let cancel_err = HalError::VcpuCancel("invalid handle".to_string());
        assert_eq!(
            cancel_err.to_string(),
            "failed to cancel vCPU: invalid handle"
        );

        let reg_err = HalError::RegisterAccess("read timeout".to_string());
        assert_eq!(reg_err.to_string(), "register access failed: read timeout");

        let interrupt_req_err = HalError::InterruptRequest("apic unavailable".to_string());
        assert_eq!(
            interrupt_req_err.to_string(),
            "interrupt request failed: apic unavailable"
        );

        let guest_mem_err = HalError::GuestMem {
            gpa: 0x1000,
            reason: "page fault".to_string(),
        };
        assert_eq!(
            guest_mem_err.to_string(),
            "guest memory access at GPA 0x1000: page fault"
        );

        let inject_err = HalError::InjectInterrupt("queue full".to_string());
        assert_eq!(
            inject_err.to_string(),
            "interrupt injection failed: queue full"
        );

        let unavailable_err = HalError::NotAvailable("KVM not loaded".to_string());
        assert_eq!(
            unavailable_err.to_string(),
            "hypervisor not available: KVM not loaded"
        );

        let platform_err = HalError::Platform {
            code: 0x1F,
            message: "device busy".to_string(),
        };
        assert_eq!(
            platform_err.to_string(),
            "platform error (code 0x1f): device busy"
        );
    }
}
