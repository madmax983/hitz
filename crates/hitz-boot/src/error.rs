//! Boot error types.
//!
//! # Abstract
//!
//! This module defines the `BootError` enum, which represents failures that
//! can occur during Linux direct boot setup (e.g., ELF loading, page table creation).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_boot::BootError;
//!
//! fn load_kernel() -> Result<(), BootError> {
//!     Err(BootError::InvalidElf("Missing magic bytes".to_string()))
//! }
//!
//! match load_kernel() {
//!     Ok(_) => println!("Kernel loaded!"),
//!     Err(BootError::InvalidElf(reason)) => println!("Bad kernel image: {}", reason),
//!     Err(e) => println!("Other boot error: {}", e),
//! }
//! ```

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
