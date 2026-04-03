//! Hypervisor Abstraction Layer for Hitz.
//!
//! # Abstract
//!
//! Platform-agnostic traits that define the contract between the VMM
//! and any hypervisor backend (WHP, KVM, etc.). This crate has zero
//! platform dependencies — it's pure trait definitions, newtypes, and errors.
//!
//! This crate serves as the central boundary layer. By depending on `hitz-hal`,
//! other components (like virtual devices or memory managers) can interact with
//! a virtual machine without needing to know if they are running on Windows
//! Hypervisor Platform (WHP) or Linux KVM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_hal::{MemSizeMiB, PartitionConfig};
//!
//! let config = PartitionConfig {
//!     vcpu_count: 4,
//!     memory_size: MemSizeMiB::new(1024),
//! };
//! // A hypervisor implementation would use this config to spin up a VM!
//! ```

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
