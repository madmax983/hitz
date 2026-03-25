//! VM lifecycle management for Hitz.
//!
//! Orchestrates vCPU threads, guest memory, boot sequence, and device I/O.

// Guest memory uses raw pointers from VirtualAlloc; this is inherent to
// the design and reviewed for safety.
#![allow(unsafe_code)]

pub(crate) mod boot_regs;
pub(crate) mod cpio;
pub(crate) mod error;
pub(crate) mod memory;
pub(crate) mod mmio_decode;
pub(crate) mod run_loop;
pub(crate) mod serial_buf;
pub(crate) mod vm;
pub(crate) mod vsock_io;

pub use boot_regs::{GDT_GPA, configure_regs, configure_sregs, write_gdt};
pub use cpio::CpioBuilder;
pub use error::MemError;
pub use memory::GuestMemory;
pub use run_loop::{ExitReason, SharedDevices, run_vcpu_loop};
pub use serial_buf::{SerialBuf, SerialReader};
pub use vm::{BootExtras, VmError, VmRunResult, boot_and_run, validate_config};
pub use vsock_io::VsockIoHandle;
