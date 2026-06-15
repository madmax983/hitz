//! Boot error types.

/// Errors that can occur during Linux direct boot setup.
///
/// # Abstract
///
/// Encapsulates failures that can happen when setting up the initial
/// guest state, such as invalid ELFs, out-of-bounds memory writes,
/// or malformed page tables.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_boot::BootError;
///
/// let err = BootError::InvalidElf("magic number mismatch".to_string());
///
/// match err {
///     BootError::InvalidElf(msg) => println!("Failed to load kernel: {}", msg),
///     _ => println!("Other boot error"),
/// }
/// ```
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
