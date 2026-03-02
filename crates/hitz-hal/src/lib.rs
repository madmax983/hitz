//! Hypervisor Abstraction Layer for Hitz.
//!
//! Platform-agnostic traits that define the contract between the VMM
//! and any hypervisor backend (WHP, KVM, etc.). This crate has zero
//! platform dependencies — it's pure trait definitions, newtypes, and errors.

// HAL traits necessarily declare unsafe methods (raw pointer parameters for
// memory mapping). The *implementations* still get warned about unsafe blocks.
#![allow(unsafe_code)]

mod error;
mod newtypes;
mod traits;
mod types;

pub use error::HalError;
pub use newtypes::{DiskOffset, Gpa, IrqLine, MacAddress, MemSizeMiB, MmioSlot, VcpuId, VmId};
pub use traits::{GuestMemAccess, Hypervisor, Partition, Vcpu};
pub use types::{
    DescriptorTable, InterruptRequest, IoPortExit, MemFlags, MmioExit, PartitionConfig,
    SegmentDescriptor, SpecialRegs, StandardRegs, VcpuExit,
};
