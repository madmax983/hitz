//! Minimal newc (SVR4) cpio archive builder.
//!
//! Produces archives compatible with the Linux kernel's initramfs loader.
//! Reference: `Documentation/driver-api/early-userspace/buffer-format.rst`
//!
//! Format: each entry = 110-byte ASCII header + filename (null-term, 4-byte
//! padded) + file data (4-byte padded). Ends with a TRAILER!!! entry.

use std::io::Write;

/// Builds a newc cpio archive in memory.
pub struct CpioBuilder {
    data: Vec<u8>,
    inode: u32,
}

impl CpioBuilder {
    /// Create an empty builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            inode: 1,
        }
    }

    /// Add a regular file entry.
    ///
    /// * `path` — file path inside the archive (no leading `/`)
    /// * `content` — file bytes
    /// * `mode` — Unix permission bits (e.g. `0o755`)
    #[must_use]
    pub fn add_file(mut self, path: &str, content: &[u8], mode: u32) -> Self {
        self.append_entry(path, content, 0o100_000 | (mode & 0o7777));
        self
    }

    /// Finish the archive by appending the TRAILER!!! entry.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.append_entry("TRAILER!!!", &[], 0);
        self.data
    }

    fn append_entry(&mut self, name: &str, content: &[u8], mode: u32) {
        let namesize = name.len() + 1; // include null terminator
        let filesize = content.len();
        // Both fields are 32-bit in the cpio spec. Inputs are always small
        // (initramfs paths and content), so truncation is safe.
        #[allow(clippy::cast_possible_truncation)]
        let namesize32 = namesize as u32;
        #[allow(clippy::cast_possible_truncation)]
        let filesize32 = filesize as u32;

        // 110-byte ASCII header.
        let start_len = self.data.len();
        let _ = write!(
            &mut self.data,
            "070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
            self.inode, // ino
            mode,       // mode
            0u32,       // uid
            0u32,       // gid
            1u32,       // nlink
            0u32,       // mtime
            filesize32, // filesize
            0u32,       // devmajor
            0u32,       // devminor
            0u32,       // rdevmajor
            0u32,       // rdevminor
            namesize32, // namesize
            0u32,       // check (always 0 for newc)
        );
        assert_eq!(
            self.data.len() - start_len,
            110,
            "cpio header must be 110 bytes"
        );

        self.inode += 1;

        // Filename + null terminator, padded to 4-byte boundary.
        self.data.extend_from_slice(name.as_bytes());
        self.data.push(0u8);
        let name_total = 110 + namesize;
        let name_pad = (4 - name_total % 4) % 4;
        self.data.extend(std::iter::repeat_n(0u8, name_pad));

        // File content, padded to 4-byte boundary.
        self.data.extend_from_slice(content);
        let data_pad = (4 - filesize % 4) % 4;
        self.data.extend(std::iter::repeat_n(0u8, data_pad));
    }
}

impl Default for CpioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_archive_has_trailer() {
        let archive = CpioBuilder::new().finish();
        // Minimum valid cpio: just the TRAILER!!! entry.
        assert!(archive.len() >= 110); // header(110) for TRAILER
        // Magic bytes at offset 0.
        assert_eq!(&archive[0..6], b"070701");
    }

    #[test]
    fn single_file_roundtrip() {
        let content = b"#!/bin/sh\n/sbin/hitz-agent &\n";
        let archive = CpioBuilder::new()
            .add_file("etc/init.d/S99hitz-agent", content, 0o755)
            .finish();
        // Verify magic.
        assert_eq!(&archive[0..6], b"070701");
        // Archive is non-empty and contains the file content.
        assert!(archive.len() > 110 + content.len());
        // Verify content is at the correct byte offset.
        // Header(110) + name("etc/init.d/S99hitz-agent\0" = 25) + 1 pad byte = 136
        let name = "etc/init.d/S99hitz-agent";
        let name_total = 110 + name.len() + 1; // +1 for null terminator
        let name_pad = (4 - name_total % 4) % 4;
        let data_start = name_total + name_pad;
        assert_eq!(&archive[data_start..data_start + content.len()], content);
    }

    #[test]
    fn two_files_both_present() {
        let archive = CpioBuilder::new()
            .add_file("sbin/hitz-agent", b"ELF", 0o755)
            .add_file("etc/init.d/S99hitz-agent", b"#!/bin/sh\n", 0o755)
            .finish();
        // Check both filenames appear in the archive bytes.
        let archive_str = String::from_utf8_lossy(&archive);
        assert!(archive_str.contains("sbin/hitz-agent"));
        assert!(archive_str.contains("S99hitz-agent"));
    }

    #[test]
    fn file_mode_is_regular_file() {
        let archive = CpioBuilder::new()
            .add_file("test.txt", b"hello", 0o777)
            .finish();

        // Mode is encoded in the ASCII header at offset 14..22 (8 bytes)
        let mode_hex = std::str::from_utf8(&archive[14..22]).expect("valid utf8");
        let parsed_mode = u32::from_str_radix(mode_hex, 16).expect("valid hex");

        // Ensure 0o100_000 (S_IFREG) is set
        assert_eq!(parsed_mode & 0o100_000, 0o100_000);
        // Ensure standard permissions are preserved
        assert_eq!(parsed_mode & 0o7777, 0o777);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn havoc_fuzz_cpio_builder(
            path in ".*",
            content in proptest::collection::vec(any::<u8>(), 0..1000),
            mode in any::<u32>(),
        ) {
            let _ = CpioBuilder::new().add_file(&path, &content, mode).finish();
        }
    }
}
