//! VM lifecycle management for Hitz.
//!
//! Orchestrates vCPU threads, guest memory, boot sequence, and device I/O.

// Guest memory uses raw pointers from VirtualAlloc; this is inherent to
// the design and reviewed for safety.
#![allow(unsafe_code)]

pub mod boot_regs;
pub mod error;
pub mod memory;
pub mod mmio_decode;
pub mod run_loop;
pub mod vm;

pub use boot_regs::{GDT_GPA, configure_regs, configure_sregs, write_gdt};
pub use error::MemError;
pub use memory::GuestMemory;
pub use run_loop::{ExitReason, run_vcpu_loop};
pub use vm::{VmError, VmRunResult, boot_and_run, validate_config};
