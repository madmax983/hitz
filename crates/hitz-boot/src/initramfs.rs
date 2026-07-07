#![allow(clippy::similar_names)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
//! Initramfs (cpio archive) loading into guest physical memory.
//!
//! # Abstract
//!
//! The kernel unpacks the cpio archive from the address stored in
//! `boot_params.hdr.ramdisk_image` during early boot. This module provides
//! the logic to load that archive into guest RAM securely and handle alignment.
//!
//! ## Examples
//!
//! ```rust
//! use hitz_boot::{load_initramfs, BootError};
//! use hitz_hal::Gpa;
//! use hitz_boot::GuestMemWriter;
//!
//! struct MockWriter;
//! impl GuestMemWriter for MockWriter {
//!     fn write_bytes(&self, gpa: Gpa, data: &[u8]) -> Result<(), BootError> { Ok(()) }
//!     fn write_zeroes(&self, gpa: Gpa, len: usize) -> Result<(), BootError> { Ok(()) }
//! }
//!
//! let data = b"archive_data";
//! let writer = MockWriter;
//! // Load an initramfs starting right after the kernel ends
//! let result = load_initramfs(data, Gpa::new(0x20_0000), 0x800_0000, &writer).unwrap();
//! assert_eq!(result.gpa, Gpa::new(0x20_0000));
//! ```

use hitz_hal::Gpa;

use crate::error::BootError;
use crate::loader::GuestMemWriter;

/// Result of loading an initramfs into guest memory.
#[derive(Debug, Clone, Copy)]
pub struct InitramfsLoadResult {
    /// Guest physical address where the initramfs was loaded.
    pub gpa: Gpa,
    /// Size of the initramfs in bytes.
    pub size: u64,
}

/// Load an initramfs (cpio archive) into guest memory after the kernel.
///
/// Places the archive at a page-aligned GPA after `kernel_end`, ensuring
/// it fits within `ram_size`.
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::{load_initramfs, BootError};
/// use hitz_hal::Gpa;
/// use hitz_boot::GuestMemWriter;
///
/// struct MockWriter;
/// impl GuestMemWriter for MockWriter {
///     fn write_bytes(&self, gpa: Gpa, data: &[u8]) -> Result<(), BootError> { Ok(()) }
///     fn write_zeroes(&self, gpa: Gpa, len: usize) -> Result<(), BootError> { Ok(()) }
/// }
///
/// let archive_data = vec![0u8; 1024];
/// let writer = MockWriter;
/// let res = load_initramfs(&archive_data, Gpa::new(0x10_0100), 0x800_0000, &writer).unwrap();
/// // Aligns up to the next 4KiB page boundary!
/// assert_eq!(res.gpa, Gpa::new(0x10_1000));
/// ```
///
/// ```compile_fail
/// use hitz_boot::load_initramfs;
/// load_initramfs("wrong_type", 123, 456, &writer);
/// ```
///
/// # Errors
///
/// Returns `BootError::InvalidBootParams` if:
/// - `data` is empty
/// - The initramfs would exceed available RAM
pub fn load_initramfs(
    data: &[u8],
    kernel_end: Gpa,
    ram_size: u64,
    writer: &impl GuestMemWriter,
) -> Result<InitramfsLoadResult, BootError> {
    if data.is_empty() {
        return Err(BootError::InvalidBootParams(
            "initramfs data is empty".into(),
        ));
    }

    // Page-align the load address (4 KiB boundary after kernel end).
    let initramfs_gpa = (kernel_end.as_u64() + 0xFFF) & !0xFFF;
    let end_gpa = initramfs_gpa + data.len() as u64;

    if end_gpa > ram_size {
        return Err(BootError::InvalidBootParams(format!(
            "initramfs at {initramfs_gpa:#x} + {:#x} bytes exceeds RAM size {ram_size:#x}",
            data.len()
        )));
    }

    writer.write_bytes(Gpa::new(initramfs_gpa), data)?;

    Ok(InitramfsLoadResult {
        gpa: Gpa::new(initramfs_gpa),
        size: data.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Mock writer that records writes for verification.
    struct MockWriter {
        writes: RefCell<Vec<(u64, Vec<u8>)>>,
    }

    impl MockWriter {
        fn new() -> Self {
            Self {
                writes: RefCell::new(Vec::new()),
            }
        }

        fn writes(&self) -> Vec<(u64, Vec<u8>)> {
            self.writes.borrow().clone()
        }
    }

    impl GuestMemWriter for MockWriter {
        fn write_bytes(&self, gpa: Gpa, data: &[u8]) -> Result<(), BootError> {
            self.writes.borrow_mut().push((gpa.as_u64(), data.to_vec()));
            Ok(())
        }

        fn write_zeroes(&self, gpa: Gpa, len: usize) -> Result<(), BootError> {
            self.writes
                .borrow_mut()
                .push((gpa.as_u64(), vec![0u8; len]));
            Ok(())
        }
    }

    #[test]
    fn empty_data_error() {
        let writer = MockWriter::new();
        let err = load_initramfs(&[], Gpa::new(0x20_0000), 0x800_0000, &writer).unwrap_err();
        assert!(err.to_string().contains("empty"), "unexpected error: {err}");
    }

    #[test]
    fn exceeds_ram_error() {
        let writer = MockWriter::new();
        let data = vec![0xABu8; 4096];
        // kernel_end at 0x7FF_F000, RAM is 0x800_0000 (128 MiB).
        // Page-aligned start = 0x7FF_F000, end = 0x800_0000 + would be 0x800_0000.
        // But 0x7FF_F000 + 4096 = 0x800_0000 which == ram_size, not >.
        // Use a tighter bound: RAM = 0x7FF_F000 + 100 (less than 4096).
        let ram_size = 0x7FF_F000 + 100;
        let err = load_initramfs(&data, Gpa::new(0x7FF_F000), ram_size, &writer).unwrap_err();
        assert!(
            err.to_string().contains("exceeds RAM"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn page_alignment() {
        let writer = MockWriter::new();
        let data = vec![0xCDu8; 512];
        // kernel_end is NOT page-aligned: 0x10_0100 (256 bytes into a page).
        let result =
            load_initramfs(&data, Gpa::new(0x10_0100), 0x800_0000, &writer).expect("should work");

        // Should round up to next page: 0x10_1000.
        assert_eq!(result.gpa, Gpa::new(0x10_1000));
        assert_eq!(result.size, 512);
    }

    #[test]
    fn successful_load() {
        let writer = MockWriter::new();
        let data = b"CPIO_ARCHIVE_DATA_HERE";
        let kernel_end = Gpa::new(0x20_0000);
        let ram_size = 0x800_0000; // 128 MiB

        let result =
            load_initramfs(data, kernel_end, ram_size, &writer).expect("load should succeed");

        // kernel_end is already page-aligned, so initramfs starts there.
        assert_eq!(result.gpa, Gpa::new(0x20_0000));
        assert_eq!(result.size, data.len() as u64);

        let writes = writer.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, 0x20_0000);
        assert_eq!(writes[0].1, data.to_vec());
    }

    #[test]
    fn kernel_end_already_aligned() {
        let writer = MockWriter::new();
        let data = vec![0xEFu8; 256];
        // kernel_end is exactly page-aligned.
        let kernel_end = Gpa::new(0x30_0000);

        let result =
            load_initramfs(&data, kernel_end, 0x800_0000, &writer).expect("should succeed");

        // No extra padding -- initramfs starts right at kernel_end.
        assert_eq!(result.gpa, Gpa::new(0x30_0000));
        assert_eq!(result.size, 256);

        let writes = writer.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, 0x30_0000);
    }
}
