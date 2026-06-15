//! Linux direct boot support for Hitz.
//!
//! # Abstract
//!
//! This module provides everything required to boot a Linux kernel
//! inside a micro-VM without needing a full BIOS or UEFI firmware.
//! It handles loading the `vmlinux` ELF, building the initial `boot_params`
//! (the Linux "zero page"), constructing ACPI tables for SMP, setting up
//! identity-mapped page tables, and injecting the initramfs.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_boot::{BootParams, build_boot_params};
//! use hitz_hal::Gpa;
//!
//! // Create boot parameters for a VM with 256 MiB of RAM.
//! let bp = build_boot_params(256 * 1024 * 1024, Gpa::new(0x2_0000)).unwrap();
//!
//! // We copy it out to a local variable because `hdr` is a packed struct,
//! // which causes alignment issues if we borrow directly in assert_eq.
//! let flag = bp.hdr.boot_flag;
//! assert_eq!(flag, 0xAA55);
//! ```

pub(crate) mod acpi;
pub(crate) mod boot_params;
pub(crate) mod cpio;
pub(crate) mod error;
pub(crate) mod initramfs;
pub(crate) mod loader;
pub(crate) mod page_tables;

pub use acpi::{MADT_GPA, RSDP_GPA, XSDT_GPA, build_madt, build_rsdp, build_xsdt};
pub use boot_params::{
    BOOT_PARAMS_GPA, BootE820Entry, BootParams, CMDLINE_GPA, E820_RAM, E820_RESERVED,
    KERNEL_LOAD_GPA, SetupHeader, build_boot_params, set_acpi_rsdp, set_initramfs_params,
};
pub use cpio::CpioBuilder;
pub use error::BootError;
pub use initramfs::{InitramfsLoadResult, load_initramfs};
pub use loader::{GuestMemWriter, KernelLoadResult, load_elf};
pub use page_tables::{MemWrite, PD_BASE_GPA, PDPT_GPA, PML4_GPA, build_page_tables};
