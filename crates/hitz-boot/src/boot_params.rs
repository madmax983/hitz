#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Linux boot protocol zero page (`boot_params`) construction.
//!
//! # Abstract
//!
//! The Linux kernel expects a 4096-byte "zero page" at a well-known GPA
//! containing an E820 memory map, a setup header with magic values, and
//! pointers to the kernel command line and initrd. This module provides
//! the structures and builders to generate a compliant `boot_params` struct.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_boot::{build_boot_params, set_initramfs_params};
//! use hitz_hal::Gpa;
//!
//! // 1. Build the base params for 128 MiB of RAM.
//! let mut bp = build_boot_params(128 * 1024 * 1024, Gpa::new(0x2_0000)).unwrap();
//!
//! // 2. If an initramfs is loaded, update the headers.
//! set_initramfs_params(&mut bp, Gpa::new(0x10_0000), 4096).unwrap();
//!
//! // The zero page is now ready to be written to memory.
//! ```

use hitz_hal::Gpa;

use crate::error::BootError;

// ---- GPA constants --------------------------------------------------------

/// Guest physical address of the zero page (`boot_params`).
pub const BOOT_PARAMS_GPA: u64 = 0x7000;

/// Guest physical address of the kernel command line string.
pub const CMDLINE_GPA: u64 = 0x2_0000;

/// Guest physical address where the kernel is loaded (1 MiB).
pub const KERNEL_LOAD_GPA: u64 = 0x10_0000;

// ---- E820 memory type constants -------------------------------------------

/// Normal usable RAM.
pub const E820_RAM: u32 = 1;

/// Reserved / unusable memory region.
pub const E820_RESERVED: u32 = 2;

// ---- Structs --------------------------------------------------------------

/// A single E820 memory map entry (20 bytes, matches Linux `struct boot_e820_entry`).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BootE820Entry {
    /// Start of the memory region.
    pub addr: u64,
    /// Size of the memory region in bytes.
    pub size: u64,
    /// Memory type (see [`E820_RAM`], [`E820_RESERVED`]).
    pub type_: u32,
}

/// Linux boot protocol setup header.
///
/// This structure is at offset `0x1F1` inside `boot_params` and carries
/// magic numbers, loader hints, and pointers that the kernel reads during
/// early boot.
///
/// Packed to match the kernel's exact byte layout (no alignment padding).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupHeader {
    /// Number of setup sectors (offset 0x1F1).
    pub setup_sects: u8,
    /// Root flags (offset 0x1F2).
    pub root_flags: u16,
    /// Protected-mode code size in 16-byte paragraphs (offset 0x1F4).
    pub syssize: u32,
    /// Obsolete (offset 0x1F8).
    pub ram_size: u16,
    /// Video mode (offset 0x1FA).
    pub vid_mode: u16,
    /// Root device number (offset 0x1FC).
    pub root_dev: u16,
    /// Boot sector magic 0xAA55 (offset 0x1FE).
    pub boot_flag: u16,
    /// Jump instruction (offset 0x200).
    pub jump: u16,
    /// Magic `HdrS` = `0x53726448` (offset 0x202).
    pub header: u32,
    /// Boot protocol version (offset 0x206).
    pub version: u16,
    /// Real-mode switch helper (offset 0x208).
    pub realmode_swtch: u32,
    /// Start of setup system segment (offset 0x20C).
    pub start_sys_seg: u16,
    /// Kernel version string pointer (offset 0x20E).
    pub kernel_version: u16,
    /// Type of boot loader (offset 0x210). 0xFF = undefined.
    pub type_of_loader: u8,
    /// Boot protocol flags (offset 0x211). `LOADED_HIGH | CAN_USE_HEAP` = 0x81.
    pub loadflags: u8,
    /// Move size for setup (offset 0x212).
    pub setup_move_size: u16,
    /// 32-bit entry point (offset 0x214).
    pub code32_start: u32,
    /// Initrd load address (offset 0x218).
    pub ramdisk_image: u32,
    /// Initrd size in bytes (offset 0x21C).
    pub ramdisk_size: u32,
    /// Bootsect helper (offset 0x220).
    pub bootsect_kludge: u32,
    /// Heap end pointer (offset 0x224). Usually 0xFE00.
    pub heap_end_ptr: u16,
    /// Extended loader version (offset 0x226).
    pub ext_loader_ver: u8,
    /// Extended loader type (offset 0x227).
    pub ext_loader_type: u8,
    /// Kernel command line physical address (offset 0x228).
    pub cmd_line_ptr: u32,
    /// Maximum initrd address (offset 0x22C).
    pub initrd_addr_max: u32,
    /// Kernel alignment requirement (offset 0x230).
    pub kernel_alignment: u32,
    /// Whether kernel is relocatable (offset 0x234).
    pub relocatable_kernel: u8,
    /// Minimum alignment (offset 0x235).
    pub min_alignment: u8,
    /// Extended load flags (offset 0x236).
    pub xloadflags: u16,
    /// Maximum command line size (offset 0x238).
    pub cmdline_size: u32,
}

// `SetupHeader` spans offsets 0x1F1..0x23C = 0x4B = 75 bytes.
const _: () = assert!(size_of::<SetupHeader>() == 75);

/// Padding between end of `SetupHeader` (0x23C) and start of `e820_table` (0x2D0).
const PAD2_SIZE: usize = 0x2D0 - 0x23C; // 148

/// Padding after `e820_table` to fill out 4096 bytes.
/// `e820_table` ends at 0x2D0 + 128*20 = 0x2D0 + 0xA00 = 0xCD0.
/// Remaining = 0x1000 - 0xCD0 = 0x330 = 816.
const PAD3_SIZE: usize = 0x1000 - (0x2D0 + 128 * size_of::<BootE820Entry>());

/// The Linux "zero page" -- a 4096-byte structure placed at [`BOOT_PARAMS_GPA`]
/// that the kernel reads during early boot.
///
/// Packed to ensure the setup header lands at exactly offset 0x1F1 and the
/// E820 table at exactly offset 0x2D0 with no alignment padding.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BootParams {
    _pad0a: [u8; 0x070],
    /// Physical address of the ACPI RSDP table (offset 0x070).
    /// Set by the bootloader so the kernel can find ACPI tables without
    /// scanning the BIOS memory area. 0 = kernel uses default discovery.
    pub acpi_rsdp_addr: u64,
    _pad0b: [u8; 0x170],
    /// Number of populated E820 entries (offset 0x1E8).
    pub e820_entries: u8,
    _pad1: [u8; 8],
    /// Setup header (offset 0x1F1).
    pub hdr: SetupHeader,
    _pad2: [u8; PAD2_SIZE],
    /// E820 memory map (offset 0x2D0). Up to 128 entries.
    pub e820_table: [BootE820Entry; 128],
    _pad3: [u8; PAD3_SIZE],
}

// Compile-time size check.
const _: () = assert!(size_of::<BootParams>() == 4096);

impl Default for BootParams {
    fn default() -> Self {
        Self {
            _pad0a: [0; 0x070],
            acpi_rsdp_addr: 0,
            _pad0b: [0; 0x170],
            e820_entries: 0,
            _pad1: [0; 8],
            hdr: SetupHeader::default(),
            _pad2: [0; PAD2_SIZE],
            e820_table: [BootE820Entry::default(); 128],
            _pad3: [0; PAD3_SIZE],
        }
    }
}

impl core::fmt::Debug for BootParams {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Copy packed fields to aligned locals before formatting.
        let entries = self.e820_entries;
        let boot_flag = { self.hdr }.boot_flag;
        let header_magic = { self.hdr }.header;
        let type_of_loader = { self.hdr }.type_of_loader;
        let loadflags = { self.hdr }.loadflags;
        let cmd_line_ptr = { self.hdr }.cmd_line_ptr;
        f.debug_struct("BootParams")
            .field("e820_entries", &entries)
            .field("hdr.boot_flag", &format_args!("{boot_flag:#x}"))
            .field("hdr.header", &format_args!("{header_magic:#x}"))
            .field("hdr.type_of_loader", &format_args!("{type_of_loader:#x}"))
            .field("hdr.loadflags", &format_args!("{loadflags:#x}"))
            .field("hdr.cmd_line_ptr", &format_args!("{cmd_line_ptr:#x}"))
            .finish_non_exhaustive()
    }
}

// ---- Builder --------------------------------------------------------------

/// Build a populated [`BootParams`] for a guest with `ram_bytes` of physical
/// memory and a command line at `cmdline_gpa`.
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::build_boot_params;
/// use hitz_hal::Gpa;
///
/// let bp = build_boot_params(128 * 1024 * 1024, Gpa::new(0x2_0000)).unwrap();
/// assert_eq!(bp.e820_entries, 3);
/// ```
///
/// # Errors
///
/// Returns [`BootError::InvalidBootParams`] if `ram_bytes` is too small to
/// hold even the kernel load region (below 2 MiB).
#[allow(clippy::cast_possible_truncation)] // cmd_line_ptr is always < 4 GiB
pub fn build_boot_params(ram_bytes: u64, cmdline_gpa: Gpa) -> Result<BootParams, BootError> {
    if ram_bytes < 0x20_0000 {
        return Err(BootError::InvalidBootParams(format!(
            "ram_bytes ({ram_bytes:#x}) must be at least 2 MiB"
        )));
    }

    let mut bp = BootParams::default();

    // Setup header magic values.
    bp.hdr.boot_flag = 0xAA55;
    bp.hdr.header = 0x5372_6448; // "HdrS"
    bp.hdr.type_of_loader = 0xFF;
    bp.hdr.loadflags = 0x81; // LOADED_HIGH | CAN_USE_HEAP
    bp.hdr.heap_end_ptr = 0xFE00;
    bp.hdr.cmd_line_ptr = cmdline_gpa.as_u64() as u32;
    bp.hdr.cmdline_size = 255;

    // E820 memory map: conventional + EBDA reserved + main RAM.
    bp.e820_table[0] = BootE820Entry {
        addr: 0,
        size: 0x9FC00,
        type_: E820_RAM,
    };
    bp.e820_table[1] = BootE820Entry {
        addr: 0x9FC00,
        size: 0x400,
        type_: E820_RESERVED,
    };
    bp.e820_table[2] = BootE820Entry {
        addr: 0x10_0000,
        size: ram_bytes - 0x10_0000,
        type_: E820_RAM,
    };
    bp.e820_entries = 3;

    Ok(bp)
}

/// Set the initramfs address and size in `boot_params`.
///
/// Updates `hdr.ramdisk_image` and `hdr.ramdisk_size` so the kernel
/// knows where to find the cpio archive in guest memory.
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::{BootParams, set_initramfs_params};
/// use hitz_hal::Gpa;
///
/// let mut bp = BootParams::default();
/// set_initramfs_params(&mut bp, Gpa::new(0x30_0000), 8192).unwrap();
/// let img = { bp.hdr }.ramdisk_image;
/// assert_eq!(img, 0x30_0000);
/// let sz = { bp.hdr }.ramdisk_size;
/// assert_eq!(sz, 8192);
/// ```
///
/// # Errors
///
/// Returns `BootError::InvalidBootParams` if GPA or size exceeds `u32::MAX`
/// (Linux boot protocol limitation).
#[allow(clippy::cast_possible_truncation)]
pub fn set_initramfs_params(bp: &mut BootParams, gpa: Gpa, size: u64) -> Result<(), BootError> {
    let gpa_val = gpa.as_u64();
    if gpa_val > u64::from(u32::MAX) {
        return Err(BootError::InvalidBootParams(format!(
            "initramfs GPA {gpa_val:#x} exceeds 32-bit limit"
        )));
    }
    if size > u64::from(u32::MAX) {
        return Err(BootError::InvalidBootParams(format!(
            "initramfs size {size:#x} exceeds 32-bit limit"
        )));
    }
    bp.hdr.ramdisk_image = gpa_val as u32;
    bp.hdr.ramdisk_size = size as u32;
    Ok(())
}

/// Set the ACPI RSDP address in `boot_params`.
///
/// When non-zero, the kernel uses this address instead of scanning
/// the BIOS area for the RSDP signature.
pub const fn set_acpi_rsdp(bp: &mut BootParams, rsdp_gpa: u64) {
    bp.acpi_rsdp_addr = rsdp_gpa;
}

// ---- Tests ----------------------------------------------------------------

impl BootParams {
    /// Return the byte representation of the boot parameters struct.
    ///
    /// # Safety
    ///
    /// `BootParams` is `repr(C, packed)` and consists solely of primitive types
    /// and arrays. Therefore, casting it to a slice of bytes is perfectly safe
    /// and does not expose uninitialized padding bytes (there are none).
    #[allow(unsafe_code)]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                std::ptr::from_ref(self).cast::<u8>(),
                std::mem::size_of::<Self>(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_params_size() {
        assert_eq!(size_of::<BootParams>(), 4096);
    }

    #[test]
    fn e820_entry_size() {
        assert_eq!(size_of::<BootE820Entry>(), 20);
    }

    #[test]
    fn setup_header_size() {
        assert_eq!(size_of::<SetupHeader>(), 75);
    }

    /// Extract the addr/size/type fields from a packed `BootE820Entry`.
    fn unpack_e820(e: BootE820Entry) -> (u64, u64, u32) {
        let addr = { e }.addr;
        let size = { e }.size;
        let type_ = { e }.type_;
        (addr, size, type_)
    }

    #[test]
    fn boot_params_e820() {
        let ram = 128 * 1024 * 1024; // 128 MiB
        let bp =
            build_boot_params(ram, Gpa::new(CMDLINE_GPA)).expect("build_boot_params should work");

        assert_eq!(bp.e820_entries, 3);

        // Entry 0: conventional RAM.
        let (addr, size, ty) = unpack_e820(bp.e820_table[0]);
        assert_eq!(addr, 0);
        assert_eq!(size, 0x9FC00);
        assert_eq!(ty, E820_RAM);

        // Entry 1: EBDA reserved.
        let (addr, size, ty) = unpack_e820(bp.e820_table[1]);
        assert_eq!(addr, 0x9FC00);
        assert_eq!(size, 0x400);
        assert_eq!(ty, E820_RESERVED);

        // Entry 2: main RAM starting at 1 MiB.
        let (addr, size, ty) = unpack_e820(bp.e820_table[2]);
        assert_eq!(addr, 0x10_0000);
        assert_eq!(size, ram - 0x10_0000);
        assert_eq!(ty, E820_RAM);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn setup_header_fields() {
        let bp = build_boot_params(128 * 1024 * 1024, Gpa::new(CMDLINE_GPA))
            .expect("build_boot_params should work");

        // Copy individual fields out of packed struct.
        let boot_flag = { bp.hdr }.boot_flag;
        let header = { bp.hdr }.header;
        let type_of_loader = { bp.hdr }.type_of_loader;
        let loadflags = { bp.hdr }.loadflags;
        let heap_end_ptr = { bp.hdr }.heap_end_ptr;
        let cmd_line_ptr = { bp.hdr }.cmd_line_ptr;
        let cmdline_size = { bp.hdr }.cmdline_size;

        assert_eq!(boot_flag, 0xAA55);
        assert_eq!(header, 0x5372_6448);
        assert_eq!(type_of_loader, 0xFF);
        assert_eq!(loadflags, 0x81);
        assert_eq!(heap_end_ptr, 0xFE00);
        assert_eq!(cmd_line_ptr, CMDLINE_GPA as u32);
        assert_eq!(cmdline_size, 255);
    }

    #[test]
    fn boot_params_too_small_ram() {
        let result = build_boot_params(0x10_0000, Gpa::new(CMDLINE_GPA));
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn set_initramfs_params_works() {
        let mut bp = BootParams::default();
        let gpa = Gpa::new(0x20_0000);
        let size = 0x1_0000_u64;

        set_initramfs_params(&mut bp, gpa, size).expect("should succeed");

        let ramdisk_image = { bp.hdr }.ramdisk_image;
        let ramdisk_size = { bp.hdr }.ramdisk_size;
        assert_eq!(ramdisk_image, 0x20_0000);
        assert_eq!(ramdisk_size, 0x1_0000);
    }

    #[test]
    fn set_initramfs_params_gpa_too_large() {
        let mut bp = BootParams::default();
        let gpa = Gpa::new(u64::from(u32::MAX) + 1);
        let err = set_initramfs_params(&mut bp, gpa, 4096).unwrap_err();
        assert!(
            err.to_string().contains("32-bit limit"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn set_initramfs_params_size_too_large() {
        let mut bp = BootParams::default();
        let gpa = Gpa::new(0x20_0000);
        let size = u64::from(u32::MAX) + 1;
        let err = set_initramfs_params(&mut bp, gpa, size).unwrap_err();
        assert!(
            err.to_string().contains("32-bit limit"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn set_acpi_rsdp_works() {
        let mut bp = BootParams::default();
        let val = { bp.acpi_rsdp_addr };
        assert_eq!(val, 0);
        set_acpi_rsdp(&mut bp, 0x000E_0000);
        let val = { bp.acpi_rsdp_addr };
        assert_eq!(val, 0x000E_0000);
    }

    #[test]
    fn acpi_rsdp_addr_offset() {
        // Verify the field is at the correct offset (0x070) within the struct.
        let bp = BootParams::default();
        let base = core::ptr::addr_of!(bp) as usize;
        let field = core::ptr::addr_of!(bp.acpi_rsdp_addr) as usize;
        assert_eq!(
            field - base,
            0x070,
            "acpi_rsdp_addr must be at offset 0x070"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::field_reassign_with_default)]
    fn boot_params_debug_format() {
        let mut bp = BootParams::default();
        bp.e820_entries = 2;
        bp.hdr.boot_flag = 0xAA55;
        bp.hdr.header = 0x5372_6448;
        bp.hdr.type_of_loader = 0xFF;
        bp.hdr.loadflags = 0x81;
        bp.hdr.cmd_line_ptr = 0x1234;

        let debug_str = format!("{bp:?}");
        assert!(debug_str.contains("e820_entries: 2"));
        assert!(debug_str.contains("boot_flag: 0xaa55"));
        assert!(debug_str.contains("header: 0x53726448"));
        assert!(debug_str.contains("type_of_loader: 0xff"));
        assert!(debug_str.contains("loadflags: 0x81"));
        assert!(debug_str.contains("cmd_line_ptr: 0x1234"));
    }
}
