//! Linux direct boot support for Hitz.
//!
//! ELF/bzImage loading, `boot_params` construction, identity-mapped page tables.

pub mod boot_params;
pub mod error;
pub mod loader;
pub mod page_tables;

pub use boot_params::{
    BOOT_PARAMS_GPA, BootE820Entry, BootParams, CMDLINE_GPA, E820_RAM, E820_RESERVED,
    KERNEL_LOAD_GPA, SetupHeader, build_boot_params,
};
pub use error::BootError;
pub use loader::{GuestMemWriter, KernelLoadResult, load_elf};
pub use page_tables::{MemWrite, PD_BASE_GPA, PDPT_GPA, PML4_GPA, build_page_tables};
