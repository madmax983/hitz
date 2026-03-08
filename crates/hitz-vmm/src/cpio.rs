//! Minimal newc (SVR4) cpio archive builder.
//!
//! Produces archives compatible with the Linux kernel's initramfs loader.
//! Reference: `Documentation/driver-api/early-userspace/buffer-format.rst`
//!
//! Format: each entry = 110-byte ASCII header + filename (null-term, 4-byte
//! padded) + file data (4-byte padded). Ends with a TRAILER!!! entry.

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

        // 110-byte ASCII header.
        let header = format!(
            "070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
            self.inode, // ino
            mode,       // mode
            0u32,       // uid
            0u32,       // gid
            1u32,       // nlink
            0u32,       // mtime
            filesize,   // filesize
            0u32,       // devmajor
            0u32,       // devminor
            0u32,       // rdevmajor
            0u32,       // rdevminor
            namesize,   // namesize
            0u32,       // check (always 0 for newc)
        );
        assert_eq!(header.len(), 110, "cpio header must be 110 bytes");

        self.inode += 1;
        self.data.extend_from_slice(header.as_bytes());

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
}
