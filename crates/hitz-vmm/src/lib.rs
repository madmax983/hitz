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
//! use hitz_api::VmConfig;
//! use hitz_vmm::{boot_and_run, VmRunResult, BootExtras, ExitReason};
//! use std::sync::atomic::{AtomicBool, Ordering};
//! use std::sync::Arc;
//! use hitz_hal::{Hypervisor, Partition, PartitionConfig, Vcpu, VcpuId, MemFlags, Gpa};
//! # struct DummyVcpu;
//! # impl Vcpu for DummyVcpu {
//! #     type CancelHandle = ();
//! #     fn get_regs(&self) -> Result<hitz_hal::StandardRegs, hitz_hal::HalError> { unimplemented!() }
//! #     fn set_regs(&mut self, _r: &hitz_hal::StandardRegs) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, hitz_hal::HalError> { unimplemented!() }
//! #     fn set_sregs(&mut self, _s: &hitz_hal::SpecialRegs) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn run(&mut self) -> Result<hitz_hal::VcpuExit, hitz_hal::HalError> { unimplemented!() }
//! #     fn cancel_via(_h: &Self::CancelHandle) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn cancel_handle(&self) -> Self::CancelHandle { unimplemented!() }
//! #     fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! # }
//! # struct DummyPartition;
//! # impl Partition for DummyPartition {
//! #     type Vcpu = DummyVcpu;
//! #     fn create_vcpu(&mut self, _id: VcpuId) -> Result<Self::Vcpu, hitz_hal::HalError> { unimplemented!() }
//! #     unsafe fn map_memory(&mut self, _gpa: Gpa, _hva: *mut u8, _size: usize, _flags: MemFlags) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn unmap_memory(&mut self, _gpa: Gpa, _size: usize) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! #     fn request_interrupt(&self, _vcpu_id: VcpuId, _vector: u8) -> Result<(), hitz_hal::HalError> { unimplemented!() }
//! # }
//! # struct DummyHypervisor;
//! # impl Hypervisor for DummyHypervisor {
//! #     type Partition = DummyPartition;
//! #     fn create_partition(&self, _config: &PartitionConfig) -> Result<Self::Partition, hitz_hal::HalError> { unimplemented!() }
//! # }
//!
//! // 1. Define the VM configuration
//! let config = VmConfig {
//!     kernel_path: "vmlinux".into(),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 256,
//!     cpus: 1,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: hitz_api::DEFAULT_GUEST_CID,
//!     guest_agent: hitz_api::GuestAgentMode::Auto,
//! };
//!
//! // 2. Create a cancellation flag
//! let cancel = Arc::new(AtomicBool::new(false));
//!
//! // 3. Boot and run the VM
//! // In practice, you would provide paths to the kernel and initramfs.
//! let hypervisor = DummyHypervisor;
//! let extras = BootExtras::none();
//! let result = boot_and_run(&hypervisor, &config, std::io::sink(), cancel, extras);
//! match result {
//!     Ok(VmRunResult { exit_reason: ExitReason::Canceled }) => println!("VM was forcefully stopped"),
//!     Ok(res) => println!("VM gracefully exited: {:?}", res.exit_reason),
//!     Err(e) => eprintln!("Failed to run VM: {:?}", e),
//! }
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
pub(crate) mod cpio;
pub(crate) mod error;
pub mod memory;
pub(crate) mod mmio_decode;
pub(crate) mod run_loop;
pub mod serial_buf;
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
/// havoc
#[cfg(test)]
pub(crate) mod havoc;
