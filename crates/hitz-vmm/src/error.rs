//! VMM error types.

/// Errors arising from guest memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MemError {
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
