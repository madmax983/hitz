//! Boot error types.

/// Errors that can occur during Linux direct boot setup.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// The ELF binary is invalid or unsupported.
    #[error("invalid ELF: {0}")]
    InvalidElf(String),

    /// A segment could not be loaded into guest memory.
    #[error("failed to load segment at GPA {gpa:#x}, size {size}: {reason}")]
    LoadSegment {
        /// Guest physical address of the segment.
        gpa: u64,
        /// Size in bytes.
        size: usize,
        /// Human-readable reason.
        reason: String,
    },

    /// A write to guest memory failed.
    #[error("write to guest memory failed: {0}")]
    WriteFailed(String),

    /// The boot params structure is invalid.
    #[error("invalid boot params: {0}")]
    InvalidBootParams(String),

    /// The page table configuration is invalid.
    #[error("invalid page table config: {0}")]
    InvalidPageTableConfig(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_error_display() {
        let err = BootError::InvalidElf("bad magic".to_string());
        assert_eq!(err.to_string(), "invalid ELF: bad magic");

        let err = BootError::LoadSegment {
            gpa: 0x1000,
            size: 4096,
            reason: "no memory".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to load segment at GPA 0x1000, size 4096: no memory"
        );

        let err = BootError::WriteFailed("io error".to_string());
        assert_eq!(err.to_string(), "write to guest memory failed: io error");

        let err = BootError::InvalidBootParams("missing field".to_string());
        assert_eq!(err.to_string(), "invalid boot params: missing field");

        let err = BootError::InvalidPageTableConfig("bad level".to_string());
        assert_eq!(err.to_string(), "invalid page table config: bad level");
    }
}
