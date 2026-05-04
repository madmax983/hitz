//! VM lifecycle management for Hitz.
//!
//! # Abstract
//!
//! This module serves as the orchestrator for the Hitz micro-VM lifecycle. It bridges
//! the gap between abstract configuration (found in `hitz-api`) and raw hypervisor
//! execution (provided by `hitz-whp` and `hitz-hal`).
//!
//! By handling vCPU threads, guest memory allocation, the Linux boot sequence (including
//! setting up page tables via `boot_regs`), and routing device I/O through a shared
//! memory bus, it ensures a seamless experience when booting micro-VMs.
//!
//! # The Hero's Journey
//!
//! To start a VM, one typically uses the [`boot_and_run`] function, supplying a configuration
//! and necessary file paths. Here is a conceptual snippet showing how the VM boot sequence
//! operates under the hood:
//!
//! ```no_run
//! use crate::config::VmConfig;
//! use hitz_vmm::{boot_and_run, VmRunResult};
//! use std::sync::atomic::{AtomicBool, Ordering};
//! use std::sync::Arc;
//!
//! // 1. Define the VM configuration
//! let config = VmConfig {
//!     ram_mib: 256,
//!     cpus: 1,
//!     kernel_path: std::path::PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     cmdline: "console=ttyS0".to_string(),
//!     net: None,
//!     guest_cid: 3,
//!     guest_agent: Default::default(),
//! };
//!
//! // 2. Create a cancellation flag
//! let cancel = Arc::new(AtomicBool::new(false));
//!
//! // 3. Boot and run the VM
//! // In practice, you would provide paths to the kernel and initramfs.
//! // let result = boot_and_run::<HitzWhpHypervisor, _>(&config, &config, None, cancel, BootExtras::default());
//! // match result {
//! //     Ok(VmRunResult::Exited(reason)) => println!("VM gracefully exited: {:?}", reason),
//! //     Ok(VmRunResult::Cancelled) => println!("VM was forcefully stopped"),
//! //     Err(e) => eprintln!("Failed to run VM: {:?}", e),
//! // }
//! ```
//!
//! # The Fine Print
//!
//! - **Guest Memory**: Guest memory uses raw pointers from Windows `VirtualAlloc`. This is
//!   inherent to the design of the Windows Hypervisor Platform (WHP) and requires `unsafe`
//!   code. Memory must be carefully mapped and unmapped to avoid access violations.
//! - **CPU Limits**: The number of vCPUs is bounded by the host's capabilities and WHP
//!   limitations. Requesting more CPUs than supported will result in a [`VmError`].
//!
//! ## Panics
//!
//! The vCPU loop inside [`run_vcpu_loop`] is designed to be resilient, but it may panic
//! if critical hypervisor state becomes corrupt (e.g., if WHP APIs return unrecoverable errors
//! during state synchronization).

// Guest memory uses raw pointers from VirtualAlloc; this is inherent to
// the design and reviewed for safety.
#![allow(unsafe_code)]

pub(crate) mod boot_regs;
pub(crate) mod error;
pub(crate) mod memory;
pub(crate) mod mmio_decode;
mod run_loop;
pub(crate) mod serial_buf;
/// Configuration module
pub mod config;
pub(crate) mod vm;
pub(crate) mod vsock_io;

pub use boot_regs::{GDT_GPA, configure_regs, configure_sregs, write_gdt};
pub use error::MemError;
pub use memory::GuestMemory;
pub use run_loop::{ExitReason, SharedDevices, run_vcpu_loop};
pub use serial_buf::{SerialBuf, SerialReader};
pub use vm::{BootExtras, VmError, VmRunResult, boot_and_run, validate_config};
pub use config::{GuestAgentMode, NetConfig, VmConfig};
pub use vsock_io::VsockIoHandle;
