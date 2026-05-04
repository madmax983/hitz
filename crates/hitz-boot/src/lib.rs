//! Linux direct boot support for Hitz.
//!
//! ELF/bzImage loading, `boot_params` construction, identity-mapped page tables.

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
