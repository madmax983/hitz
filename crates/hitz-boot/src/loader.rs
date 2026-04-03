#![allow(clippy::similar_names)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
//! Minimal ELF64 vmlinux loader.
//!
//! Parses a statically-linked ELF64 binary (such as a vmlinux) and copies
//! its `PT_LOAD` segments into guest physical memory via the [`GuestMemWriter`]
//! trait.  No external ELF crate is used -- the header structures are defined
//! inline so the crate compiles cleanly on Windows.

use hitz_hal::Gpa;

use crate::error::BootError;

// ---- ELF constants --------------------------------------------------------

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;

// ---- Parsed ELF structures ------------------------------------------------

/// Parsed ELF64 file header (only the fields we need).
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    entry: u64,
    phoff: u64,
    phentsize: u16,
    phnum: u16,
}

/// Parsed ELF64 program header (only the fields we need).
#[derive(Debug, Clone, Copy)]
struct Elf64Phdr {
    ty: u32,
    offset: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
}

// ---- Byte-level parsing helpers -------------------------------------------

/// Read a little-endian `u16` from `data` at `offset`.
fn read_u16(data: &[u8], offset: usize) -> Result<u16, BootError> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| BootError::InvalidElf(format!("truncated at offset {offset:#x}")))?;
    Ok(u16::from_le_bytes(bytes))
}

/// Read a little-endian `u32` from `data` at `offset`.
fn read_u32(data: &[u8], offset: usize) -> Result<u32, BootError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| BootError::InvalidElf(format!("truncated at offset {offset:#x}")))?;
    Ok(u32::from_le_bytes(bytes))
}

/// Read a little-endian `u64` from `data` at `offset`.
fn read_u64(data: &[u8], offset: usize) -> Result<u64, BootError> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| BootError::InvalidElf(format!("truncated at offset {offset:#x}")))?;
    Ok(u64::from_le_bytes(bytes))
}

// ---- Header parsing -------------------------------------------------------

/// Parse the ELF64 file header from the first 64 bytes.
fn parse_elf_header(data: &[u8]) -> Result<Elf64Header, BootError> {
    if data.len() < ELF_HEADER_SIZE {
        return Err(BootError::InvalidElf(format!(
            "file too small ({} bytes, need at least {ELF_HEADER_SIZE})",
            data.len()
        )));
    }

    if data[0..4] != ELF_MAGIC {
        return Err(BootError::InvalidElf("bad ELF magic".into()));
    }
    if data[4] != ELFCLASS64 {
        return Err(BootError::InvalidElf(format!(
            "not ELF64 (class = {})",
            data[4]
        )));
    }
    if data[5] != ELFDATA2LSB {
        return Err(BootError::InvalidElf(format!(
            "not little-endian (data = {})",
            data[5]
        )));
    }

    let machine = read_u16(data, 18)?;
    if machine != EM_X86_64 {
        return Err(BootError::InvalidElf(format!(
            "not x86_64 (e_machine = {machine})"
        )));
    }

    Ok(Elf64Header {
        entry: read_u64(data, 24)?,
        phoff: read_u64(data, 32)?,
        phentsize: read_u16(data, 54)?,
        phnum: read_u16(data, 56)?,
    })
}

/// Parse a single program header from `data` at byte offset `base`.
fn parse_program_header(data: &[u8], base: usize) -> Result<Elf64Phdr, BootError> {
    if data.len() < base + PROGRAM_HEADER_SIZE {
        return Err(BootError::InvalidElf(format!(
            "program header at {base:#x} extends past end of file"
        )));
    }
    Ok(Elf64Phdr {
        ty: read_u32(data, base)?,
        offset: read_u64(data, base + 8)?,
        paddr: read_u64(data, base + 24)?,
        filesz: read_u64(data, base + 32)?,
        memsz: read_u64(data, base + 40)?,
    })
}

// ---- Guest memory writer trait --------------------------------------------

/// Trait for writing data into guest physical memory.
///
/// Implemented by `GuestMemory` in `hitz-vmm`.  Defined here so that
/// `hitz-boot` does not depend on `hitz-vmm`.
pub trait GuestMemWriter {
    /// Write `data` at the given guest physical address.
    fn write_bytes(&self, gpa: Gpa, data: &[u8]) -> Result<(), BootError>;

    /// Fill `len` bytes at `gpa` with zeroes.
    fn write_zeroes(&self, gpa: Gpa, len: usize) -> Result<(), BootError>;
}

// ---- Load result ----------------------------------------------------------

/// Result of loading a kernel ELF into guest memory.
#[derive(Debug, Clone, Copy)]
pub struct KernelLoadResult {
    /// GPA of the first byte loaded (minimum `p_paddr`).
    pub kernel_load: Gpa,
    /// GPA one past the last byte loaded (maximum `p_paddr + p_memsz`).
    pub kernel_end: Gpa,
    /// Entry point address from the ELF header (`e_entry`).
    pub entry_point: Gpa,
}

// ---- Main loader ----------------------------------------------------------

/// Load an ELF64 vmlinux binary from `kernel` bytes into guest memory via
/// `writer`.
///
/// Only `PT_LOAD` segments are processed.  Each segment's file-backed
/// portion is copied verbatim; any excess `p_memsz` beyond `p_filesz`
/// (the BSS) is zero-filled.
///
/// # Errors
///
/// Returns [`BootError::InvalidElf`] if the bytes are not a valid ELF64
/// x86-64 binary, or [`BootError::LoadSegment`] / [`BootError::WriteFailed`]
/// if a guest memory write fails.
#[allow(clippy::cast_possible_truncation)] // ELF offsets fit in usize on 64-bit
pub fn load_elf(
    kernel: &[u8],
    writer: &impl GuestMemWriter,
) -> Result<KernelLoadResult, BootError> {
    let hdr = parse_elf_header(kernel)?;

    let mut min_addr: Option<u64> = None;
    let mut max_addr: u64 = 0;
    let mut loaded_any = false;

    for i in 0..hdr.phnum {
        let ph_offset = hdr
            .phoff
            .checked_add(u64::from(i) * u64::from(hdr.phentsize))
            .ok_or_else(|| BootError::InvalidElf("program header offset overflow".into()))?;

        let ph = parse_program_header(kernel, ph_offset as usize)?;

        if ph.ty != PT_LOAD {
            continue;
        }

        // Copy file-backed portion.
        let file_off = ph.offset as usize;
        let file_sz = ph.filesz as usize;

        if file_sz > 0 {
            let end = file_off
                .checked_add(file_sz)
                .ok_or_else(|| BootError::InvalidElf("segment offset overflow".into()))?;
            let data = kernel
                .get(file_off..end)
                .ok_or_else(|| BootError::LoadSegment {
                    gpa: ph.paddr,
                    size: file_sz,
                    reason: "segment data extends past end of file".into(),
                })?;
            writer.write_bytes(Gpa::new(ph.paddr), data)?;
        }

        // Zero-fill BSS (memsz > filesz).
        if ph.memsz > ph.filesz {
            let bss_start = ph
                .paddr
                .checked_add(ph.filesz)
                .ok_or_else(|| BootError::InvalidElf("segment offset overflow".into()))?;
            let bss_len = ph
                .memsz
                .checked_sub(ph.filesz)
                .ok_or_else(|| BootError::InvalidElf("segment offset overflow".into()))?
                as usize;
            writer.write_zeroes(Gpa::new(bss_start), bss_len)?;
        }

        // Track address bounds.
        let seg_end = ph
            .paddr
            .checked_add(ph.memsz)
            .ok_or_else(|| BootError::InvalidElf("segment offset overflow".into()))?;
        min_addr = Some(min_addr.map_or(ph.paddr, |cur: u64| cur.min(ph.paddr)));
        if seg_end > max_addr {
            max_addr = seg_end;
        }
        loaded_any = true;
    }

    if !loaded_any {
        return Err(BootError::InvalidElf("no PT_LOAD segments found".into()));
    }

    // We checked `loaded_any`, so `min_addr` is `Some`.
    let kernel_load = min_addr.ok_or_else(|| BootError::InvalidElf("unreachable".into()))?;

    Ok(KernelLoadResult {
        kernel_load: Gpa::new(kernel_load),
        kernel_end: Gpa::new(max_addr),
        entry_point: Gpa::new(hdr.entry),
    })
}

// ---- Tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Mock writer that records all writes for verification.
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

    /// Build a minimal valid ELF64 with one `PT_LOAD` segment containing
    /// `data` loaded at `load_addr`.
    fn make_test_elf(load_addr: u64, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE];

        // ELF header (64 bytes).
        buf[0..4].copy_from_slice(&ELF_MAGIC);
        buf[4] = ELFCLASS64;
        buf[5] = ELFDATA2LSB;
        buf[6] = 1; // EV_CURRENT
        buf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        buf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
        buf[24..32].copy_from_slice(&load_addr.to_le_bytes()); // e_entry
        buf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
        buf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        buf[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum

        // Program header at offset 64 (56 bytes).
        let data_offset = (ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE) as u64;
        let ph = ELF_HEADER_SIZE;
        buf[ph..ph + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[ph + 8..ph + 16].copy_from_slice(&data_offset.to_le_bytes()); // p_offset
        buf[ph + 16..ph + 24].copy_from_slice(&load_addr.to_le_bytes()); // p_vaddr
        buf[ph + 24..ph + 32].copy_from_slice(&load_addr.to_le_bytes()); // p_paddr
        buf[ph + 32..ph + 40].copy_from_slice(&(data.len() as u64).to_le_bytes()); // p_filesz
        buf[ph + 40..ph + 48].copy_from_slice(&(data.len() as u64).to_le_bytes()); // p_memsz

        buf.extend_from_slice(data);
        buf
    }

    /// Build a test ELF where `memsz > filesz`, creating a BSS region.
    fn make_test_elf_with_bss(load_addr: u64, data: &[u8], bss_extra: u64) -> Vec<u8> {
        let mut buf = vec![0u8; ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE];

        buf[0..4].copy_from_slice(&ELF_MAGIC);
        buf[4] = ELFCLASS64;
        buf[5] = ELFDATA2LSB;
        buf[6] = 1;
        buf[16..18].copy_from_slice(&2u16.to_le_bytes());
        buf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        buf[20..24].copy_from_slice(&1u32.to_le_bytes());
        buf[24..32].copy_from_slice(&load_addr.to_le_bytes());
        buf[32..40].copy_from_slice(&64u64.to_le_bytes());
        buf[52..54].copy_from_slice(&64u16.to_le_bytes());
        buf[54..56].copy_from_slice(&56u16.to_le_bytes());
        buf[56..58].copy_from_slice(&1u16.to_le_bytes());

        let data_offset = (ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE) as u64;
        let memsz = data.len() as u64 + bss_extra;
        let ph = ELF_HEADER_SIZE;

        buf[ph..ph + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[ph + 8..ph + 16].copy_from_slice(&data_offset.to_le_bytes());
        buf[ph + 16..ph + 24].copy_from_slice(&load_addr.to_le_bytes());
        buf[ph + 24..ph + 32].copy_from_slice(&load_addr.to_le_bytes());
        buf[ph + 32..ph + 40].copy_from_slice(&(data.len() as u64).to_le_bytes());
        buf[ph + 40..ph + 48].copy_from_slice(&memsz.to_le_bytes());

        buf.extend_from_slice(data);
        buf
    }

    #[test]
    fn elf_header_size_check() {
        assert_eq!(ELF_HEADER_SIZE, 64);
    }

    #[test]
    fn program_header_size_check() {
        assert_eq!(PROGRAM_HEADER_SIZE, 56);
    }

    #[test]
    fn load_minimal_elf() {
        let payload = b"Hello, VM!";
        let load_addr = 0x10_0000_u64;
        let elf = make_test_elf(load_addr, payload);
        let writer = MockWriter::new();

        let result = load_elf(&elf, &writer).expect("load should succeed");

        assert_eq!(result.kernel_load, Gpa::new(load_addr));
        assert_eq!(
            result.kernel_end,
            Gpa::new(load_addr + payload.len() as u64)
        );
        assert_eq!(result.entry_point, Gpa::new(load_addr));

        let writes = writer.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, load_addr);
        assert_eq!(writes[0].1, payload.to_vec());
    }

    #[test]
    fn load_elf_with_bss() {
        let payload = b"\xDE\xAD";
        let load_addr = 0x20_0000_u64;
        let bss_extra = 64_u64;
        let elf = make_test_elf_with_bss(load_addr, payload, bss_extra);
        let writer = MockWriter::new();

        let result = load_elf(&elf, &writer).expect("load should succeed");

        assert_eq!(result.kernel_load, Gpa::new(load_addr));
        assert_eq!(
            result.kernel_end,
            Gpa::new(load_addr + payload.len() as u64 + bss_extra)
        );

        let writes = writer.writes();
        // First write: file data.  Second write: BSS zeroes.
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].0, load_addr);
        assert_eq!(writes[0].1, payload.to_vec());
        assert_eq!(writes[1].0, load_addr + payload.len() as u64);
        assert_eq!(writes[1].1, vec![0u8; bss_extra as usize]);
    }

    #[test]
    fn invalid_magic() {
        let bad = vec![0u8; 128];
        let writer = MockWriter::new();
        let err = load_elf(&bad, &writer).unwrap_err();
        assert!(
            err.to_string().contains("bad ELF magic"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn empty_kernel() {
        let writer = MockWriter::new();
        let err = load_elf(&[], &writer).unwrap_err();
        assert!(
            err.to_string().contains("too small"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn wrong_machine() {
        let mut elf = make_test_elf(0x10_0000, b"x");
        // Overwrite e_machine with ARM (0x28).
        elf[18..20].copy_from_slice(&0x28u16.to_le_bytes());
        let writer = MockWriter::new();
        let err = load_elf(&elf, &writer).unwrap_err();
        assert!(
            err.to_string().contains("not x86_64"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn not_elf64() {
        let mut elf = make_test_elf(0x10_0000, b"x");
        // Change class from ELFCLASS64 (2) to ELFCLASS32 (1).
        elf[4] = 1;
        let writer = MockWriter::new();
        let err = load_elf(&elf, &writer).unwrap_err();
        assert!(
            err.to_string().contains("not ELF64"),
            "unexpected error: {err}"
        );
    }
}
