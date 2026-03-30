//! VMM error types.

/// Errors arising from guest memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MemError {
    /// Invalid size requested for memory region.
    #[error("invalid size {size} bytes at GPA {gpa:#x}")]
    InvalidSize {
        /// Intended guest physical address.
        gpa: u64,
        /// Requested size in bytes.
        size: usize,
    },

    /// Host-side allocation (`VirtualAlloc`) failed.
    #[error("allocation failed for {size} bytes at GPA {gpa:#x}")]
    AllocFailed {
        /// Intended guest physical address.
        gpa: u64,
        /// Requested size in bytes.
        size: usize,
    },

    /// The requested GPA range is not backed by any region.
    #[error("GPA {gpa:#x} + {len} bytes is not mapped")]
    NotMapped {
        /// Guest physical address.
        gpa: u64,
        /// Requested length.
        len: usize,
    },

    /// Access extends beyond the end of its containing region.
    #[error("GPA {gpa:#x} + {len} overflows region boundary")]
    OutOfBounds {
        /// Guest physical address.
        gpa: u64,
        /// Requested length.
        len: usize,
    },

    /// An error bubbled up from the HAL layer.
    #[error("HAL error: {0}")]
    Hal(#[from] hitz_hal::HalError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_format_invalid_size() {
        let err = MemError::InvalidSize {
            gpa: 0x1000,
            size: usize::MAX,
        };
        assert_eq!(
            err.to_string(),
            format!("invalid size {} bytes at GPA 0x1000", usize::MAX)
        );
    }

    #[test]
    fn should_format_alloc_failed() {
        let err = MemError::AllocFailed {
            gpa: 0x1000,
            size: 4096,
        };
        assert_eq!(
            err.to_string(),
            "allocation failed for 4096 bytes at GPA 0x1000"
        );
    }

    #[test]
    fn should_format_not_mapped() {
        let err = MemError::NotMapped {
            gpa: 0x2000,
            len: 128,
        };
        assert_eq!(err.to_string(), "GPA 0x2000 + 128 bytes is not mapped");
    }

    #[test]
    fn should_format_out_of_bounds() {
        let err = MemError::OutOfBounds {
            gpa: 0x3000,
            len: 256,
        };
        assert_eq!(
            err.to_string(),
            "GPA 0x3000 + 256 overflows region boundary"
        );
    }

    #[test]
    fn should_format_hal() {
        // Create a dummy HalError to test the display formatting
        let hal_err = hitz_hal::HalError::CreatePartition("dummy reason".to_string());
        let err = MemError::Hal(hal_err);
        assert_eq!(
            err.to_string(),
            "HAL error: failed to create partition: dummy reason"
        );
    }
}
