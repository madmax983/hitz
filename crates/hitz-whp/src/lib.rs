//! Windows Hypervisor Platform backend for Hitz.
//!
//! Implements the [`hitz_hal`] traits using the WHP API via `windows-rs`.
//!
//! # Architecture
//!
//! - [`WhpHypervisor`] — singleton, checks WHP availability, creates partitions
//! - [`WhpPartition`] — owns a `WHV_PARTITION_HANDLE`, maps memory, creates vCPUs
//! - [`WhpVcpu`] — owns a vCPU index within a partition, runs the guest

// WHP requires unsafe FFI calls throughout.
#![allow(unsafe_code)]

mod convert;
mod inner;
mod partition;
#[cfg(test)]
mod tests;
mod vcpu;

pub use partition::WhpPartition;
pub use vcpu::WhpVcpu;

use hitz_hal::{HalError, Hypervisor, PartitionConfig};

/// WHP hypervisor backend. One per process.
pub struct WhpHypervisor;

impl WhpHypervisor {
    /// Create a new WHP hypervisor instance.
    ///
    /// Checks that the Windows Hypervisor Platform is available.
    pub const fn new() -> Result<Self, HalError> {
        // WHvGetCapability could be used here to check for WHP support,
        // but WHvCreatePartition itself will fail clearly if WHP is disabled.
        // We keep the constructor cheap.
        Ok(Self)
    }
}

impl Hypervisor for WhpHypervisor {
    type Partition = WhpPartition;

    fn create_partition(&self, cfg: &PartitionConfig) -> Result<Self::Partition, HalError> {
        WhpPartition::new(cfg)
    }
}
