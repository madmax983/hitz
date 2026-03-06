//! Core HAL traits: Hypervisor → Partition → Vcpu.
//!
//! Each backend (WHP, KVM, etc.) implements these traits with its own
//! concrete types via associated types — no boxing, no dynamic dispatch
//! on the hot path.

use crate::error::HalError;
use crate::newtypes::{Gpa, VcpuId};
use crate::types::{MemFlags, PartitionConfig, SpecialRegs, StandardRegs, VcpuExit};

/// Guest physical memory access — allows devices to read/write guest RAM
/// without depending on a concrete memory implementation.
///
/// This is the key abstraction that decouples hitz-devices (virtio stack)
/// from hitz-vmm (memory management), preventing circular dependencies.
pub trait GuestMemAccess: Send + Sync {
    /// Read `buf.len()` bytes from guest physical address `gpa` into `buf`.
    fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), HalError>;

    /// Write `data` into guest memory starting at `gpa`.
    fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), HalError>;
}

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
    type Vcpu: Vcpu + 'static;

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
    /// Opaque handle that can cancel a running vCPU from any thread.
    type CancelHandle: Send + Sync + Clone;

    /// Enter the guest and run until an exit occurs.
    fn run(&mut self) -> Result<VcpuExit, HalError>;

    /// Extract a cancel handle from this vCPU.
    ///
    /// Call this before moving the vCPU into a thread. The handle can
    /// be cloned and shared freely.
    fn cancel_handle(&self) -> Self::CancelHandle;

    /// Cancel a running vCPU using a previously extracted handle.
    ///
    /// This causes `run` to return `VcpuExit::Canceled`.
    fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError>;

    /// Cancel a running vCPU from another thread.
    ///
    /// This causes `run` to return `VcpuExit::Canceled`.
    fn cancel(&self) -> Result<(), HalError> {
        Self::cancel_via(&self.cancel_handle())
    }

    /// Read the general-purpose registers.
    fn get_regs(&self) -> Result<StandardRegs, HalError>;

    /// Write the general-purpose registers.
    fn set_regs(&mut self, regs: &StandardRegs) -> Result<(), HalError>;

    /// Read the special (system) registers.
    fn get_sregs(&self) -> Result<SpecialRegs, HalError>;

    /// Write the special (system) registers.
    fn set_sregs(&mut self, sregs: &SpecialRegs) -> Result<(), HalError>;

    /// Inject an external interrupt into the vCPU.
    ///
    /// On WHP this sets `WHvRegisterPendingInterruption` before re-entering
    /// the guest. The guest must have IF=1 (interrupts enabled) for this
    /// to take effect immediately.
    fn inject_interrupt(&mut self, vector: u8) -> Result<(), HalError>;

    /// Request an `InterruptWindow` exit when the guest becomes interruptible.
    ///
    /// On WHP this sets `WHvX64RegisterDeliverabilityNotifications` bit 0,
    /// causing the next `run` to exit with `VcpuExit::InterruptWindow` once
    /// IF=1 and the vCPU is not in an interrupt shadow.
    fn request_interrupt_window(&mut self) -> Result<(), HalError>;
}
