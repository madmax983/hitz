//! Core HAL traits: Hypervisor → Partition → Vcpu.
//!
//! Each backend (WHP, KVM, etc.) implements these traits with its own
//! concrete types via associated types — no boxing, no dynamic dispatch
//! on the hot path.

use crate::error::HalError;
use crate::newtypes::{Gpa, VcpuId};
use crate::types::{MemFlags, PartitionConfig, SpecialRegs, StandardRegs, VcpuExit};

/// Top-level hypervisor interface. One per process.
pub trait Hypervisor: Send + Sync {
    /// The partition type produced by this hypervisor.
    type Partition: Partition;

    /// Create a new VM partition with the given configuration.
    fn create_partition(&self, cfg: &PartitionConfig) -> Result<Self::Partition, HalError>;
}

/// A VM partition — owns guest memory mappings and vCPUs.
///
/// # Safety
///
/// Implementations must ensure that memory mappings outlive any vCPU
/// that may access them.
pub trait Partition: Send + Sync {
    /// The vCPU type produced by this partition.
    type Vcpu: Vcpu;

    /// Map a region of host memory into the guest physical address space.
    ///
    /// # Safety
    ///
    /// `hva` must point to a valid allocation of at least `size` bytes
    /// that remains valid for the lifetime of this mapping.
    unsafe fn map_memory(
        &mut self,
        gpa: Gpa,
        hva: *mut u8,
        size: usize,
        flags: MemFlags,
    ) -> Result<(), HalError>;

    /// Remove a previously mapped region from the guest physical address space.
    fn unmap_memory(&mut self, gpa: Gpa, size: usize) -> Result<(), HalError>;

    /// Create a new virtual processor in this partition.
    fn create_vcpu(&mut self, id: VcpuId) -> Result<Self::Vcpu, HalError>;

    /// Request an interrupt be delivered to a vCPU.
    ///
    /// On WHP this requires canceling the vCPU run and using the
    /// interrupt window mechanism.
    fn request_interrupt(&self, vcpu_id: VcpuId, vector: u8) -> Result<(), HalError>;
}

/// A virtual processor — the vCPU run loop lives here.
///
/// Each vCPU runs on its own OS thread. The `run` method blocks until
/// the guest exits (I/O, MMIO, HLT, etc.).
pub trait Vcpu: Send {
    /// Enter the guest and run until an exit occurs.
    fn run(&mut self) -> Result<VcpuExit, HalError>;

    /// Cancel a running vCPU from another thread.
    ///
    /// This causes `run` to return `VcpuExit::Canceled`.
    fn cancel(&self) -> Result<(), HalError>;

    /// Read the general-purpose registers.
    fn get_regs(&self) -> Result<StandardRegs, HalError>;

    /// Write the general-purpose registers.
    fn set_regs(&mut self, regs: &StandardRegs) -> Result<(), HalError>;

    /// Read the special (system) registers.
    fn get_sregs(&self) -> Result<SpecialRegs, HalError>;

    /// Write the special (system) registers.
    fn set_sregs(&mut self, sregs: &SpecialRegs) -> Result<(), HalError>;
}
