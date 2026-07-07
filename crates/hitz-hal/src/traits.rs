//! Core HAL traits: Hypervisor → Partition → Vcpu.
//!
//! Each backend (WHP, KVM, etc.) implements these traits with its own
//! concrete types via associated types — no boxing, no dynamic dispatch
//! on the hot path.

use crate::error::HalError;
use crate::newtypes::{Gpa, VcpuId};
use crate::types::{MemFlags, PartitionConfig, SpecialRegs, StandardRegs, VcpuExit};

/// Guest physical memory access.
///
/// # Abstract
///
/// Allows devices to read/write guest RAM
/// without depending on a concrete memory implementation.
/// This is the key abstraction that decouples `hitz-devices` (virtio stack)
/// from `hitz-vmm` (memory management), preventing circular dependencies.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{GuestMemAccess, HalError};
///
/// # struct DummyMem;
/// # impl GuestMemAccess for DummyMem {
/// #     fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), HalError> { Ok(()) }
/// #     fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), HalError> { Ok(()) }
/// # }
/// # let memory = DummyMem;
/// let mut buffer = [0u8; 4];
/// // Read 4 bytes from guest physical address 0x1000.
/// memory.read_guest(0x1000, &mut buffer).unwrap();
/// ```
pub trait GuestMemAccess: Send + Sync {
    /// Read `buf.len()` bytes from guest physical address `gpa` into `buf`.
    fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), HalError>;

    /// Write `data` into guest memory starting at `gpa`.
    fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), HalError>;
}

/// Top-level hypervisor interface.
///
/// # Abstract
///
/// This is the entry point for interacting with a hypervisor. Generally,
/// there is only one instance of this per process. It acts as a factory
/// for creating [`Partition`] instances.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{Hypervisor, PartitionConfig, HalError, MemSizeMiB};
/// # use hitz_hal::{Partition, VcpuId, MemFlags, Gpa, Vcpu};
/// # struct DummyPartition;
/// # struct DummyVcpu;
/// # impl Vcpu for DummyVcpu {
/// #     type CancelHandle = ();
/// #     fn run(&mut self) -> Result<hitz_hal::VcpuExit, HalError> { unreachable!() }
/// #     fn cancel_handle(&self) -> () { () }
/// #     fn cancel_via(_: &()) -> Result<(), HalError> { Ok(()) }
/// #     fn get_regs(&self) -> Result<hitz_hal::StandardRegs, HalError> { unreachable!() }
/// #     fn set_regs(&mut self, _: &hitz_hal::StandardRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, HalError> { unreachable!() }
/// #     fn set_sregs(&mut self, _: &hitz_hal::SpecialRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _: u8) -> Result<(), HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), HalError> { Ok(()) }
/// # }
/// # impl Partition for DummyPartition {
/// #     type Vcpu = DummyVcpu;
/// #     unsafe fn map_memory(&mut self, _: Gpa, _: *mut u8, _: usize, _: MemFlags) -> Result<(), HalError> { Ok(()) }
/// #     fn unmap_memory(&mut self, _: Gpa, _: usize) -> Result<(), HalError> { Ok(()) }
/// #     fn create_vcpu(&mut self, _: VcpuId) -> Result<Self::Vcpu, HalError> { Ok(DummyVcpu) }
/// #     fn request_interrupt(&self, _: VcpuId, _: u8) -> Result<(), HalError> { Ok(()) }
/// # }
/// # struct DummyHypervisor;
/// # impl Hypervisor for DummyHypervisor {
/// #     type Partition = DummyPartition;
/// #     fn create_partition(&self, _: &PartitionConfig) -> Result<Self::Partition, HalError> {
/// #         Ok(DummyPartition)
/// #     }
/// # }
/// # let hv = DummyHypervisor;
/// let config = PartitionConfig {
///     vcpu_count: 2,
///     memory_size: MemSizeMiB::new(2048),
/// };
/// let partition = hv.create_partition(&config).expect("failed to create VM");
/// ```
pub trait Hypervisor: Send + Sync {
    /// The partition type produced by this hypervisor.
    type Partition: Partition;

    /// Create a new VM partition with the given configuration.
    fn create_partition(&self, cfg: &PartitionConfig) -> Result<Self::Partition, HalError>;
}

/// A VM partition — owns guest memory mappings and vCPUs.
///
/// # Abstract
///
/// A partition represents a single virtual machine instance. It encapsulates
/// the physical memory mappings (RAM) and acts as a factory for its
/// virtual processors ([`Vcpu`]).
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{Partition, VcpuId, HalError, Gpa, MemFlags};
/// # use hitz_hal::{Vcpu};
/// # struct DummyVcpu;
/// # impl Vcpu for DummyVcpu {
/// #     type CancelHandle = ();
/// #     fn run(&mut self) -> Result<hitz_hal::VcpuExit, HalError> { unreachable!() }
/// #     fn cancel_handle(&self) -> () { () }
/// #     fn cancel_via(_: &()) -> Result<(), HalError> { Ok(()) }
/// #     fn get_regs(&self) -> Result<hitz_hal::StandardRegs, HalError> { unreachable!() }
/// #     fn set_regs(&mut self, _: &hitz_hal::StandardRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, HalError> { unreachable!() }
/// #     fn set_sregs(&mut self, _: &hitz_hal::SpecialRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _: u8) -> Result<(), HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), HalError> { Ok(()) }
/// # }
/// # struct DummyPartition;
/// # impl Partition for DummyPartition {
/// #     type Vcpu = DummyVcpu;
/// #     unsafe fn map_memory(&mut self, _: Gpa, _: *mut u8, _: usize, _: MemFlags) -> Result<(), HalError> { Ok(()) }
/// #     fn unmap_memory(&mut self, _: Gpa, _: usize) -> Result<(), HalError> { Ok(()) }
/// #     fn create_vcpu(&mut self, _: VcpuId) -> Result<Self::Vcpu, HalError> { Ok(DummyVcpu) }
/// #     fn request_interrupt(&self, _: VcpuId, _: u8) -> Result<(), HalError> { Ok(()) }
/// # }
/// # let mut partition = DummyPartition;
/// let vcpu = partition.create_vcpu(VcpuId::new(0)).expect("failed to create vCPU 0");
/// ```
///
/// # Details
///
/// Implementations must ensure that memory mappings outlive any vCPU
/// that may access them. The `map_memory` function is `unsafe` because
/// the caller must guarantee the host pointer remains valid.
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
/// # Abstract
///
/// Each vCPU usually runs on its own dedicated OS thread. The `run` method
/// enters the guest execution context and blocks until the guest exits
/// due to an event like I/O, MMIO, or HLT.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{Vcpu, HalError, VcpuExit};
/// # struct DummyVcpu;
/// # impl Vcpu for DummyVcpu {
/// #     type CancelHandle = ();
/// #     fn run(&mut self) -> Result<hitz_hal::VcpuExit, HalError> { Ok(VcpuExit::Halt) }
/// #     fn cancel_handle(&self) -> () { () }
/// #     fn cancel_via(_: &()) -> Result<(), HalError> { Ok(()) }
/// #     fn get_regs(&self) -> Result<hitz_hal::StandardRegs, HalError> { unreachable!() }
/// #     fn set_regs(&mut self, _: &hitz_hal::StandardRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, HalError> { unreachable!() }
/// #     fn set_sregs(&mut self, _: &hitz_hal::SpecialRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _: u8) -> Result<(), HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), HalError> { Ok(()) }
/// # }
/// # let mut vcpu = DummyVcpu;
/// loop {
///     match vcpu.run().expect("vcpu run failed") {
///         VcpuExit::Halt => break, // Guest executed HLT
///         _ => {} // Handle other exits
///     }
/// }
/// ```
///
/// # Details
///
/// Call [`cancel_handle`][Vcpu::cancel_handle] *before* moving the vCPU into
/// a thread. Share the handle via `Arc<Self::CancelHandle>` or by cloning,
/// then call [`cancel_via`][Vcpu::cancel_via] from any thread to interrupt a
/// blocked `run` call.
pub trait Vcpu: Send {
    /// Opaque handle that can cancel a running vCPU from any thread.
    ///
    /// Must be extracted before the vCPU is moved into a thread.
    /// Safe to clone and share across threads via `Arc` or direct clone.
    type CancelHandle: Send + Sync + Clone;

    /// Enter the guest and run until an exit occurs.
    fn run(&mut self) -> Result<VcpuExit, HalError>;

    /// Extract a cancel handle for this vCPU.
    ///
    /// Call this *before* moving the vCPU into a thread. The returned
    /// handle can be cloned and shared freely across threads.
    fn cancel_handle(&self) -> Self::CancelHandle;

    /// Cancel a running vCPU using a previously extracted handle.
    ///
    /// Forces `run` to return `VcpuExit::Canceled`. Safe to call from
    /// any thread. Returns `Ok(())` if the cancel was delivered (the
    /// vCPU may not have exited yet).
    ///
    /// Returns `Err` if the underlying hypervisor call fails (e.g. the
    /// partition has already been destroyed).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SpecialRegs, StandardRegs, VcpuExit};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct DummyVcpu {
        was_canceled: Arc<AtomicBool>,
    }

    impl Vcpu for DummyVcpu {
        type CancelHandle = Arc<AtomicBool>;

        fn run(&mut self) -> Result<VcpuExit, HalError> {
            Ok(VcpuExit::Halt)
        }

        fn cancel_handle(&self) -> Self::CancelHandle {
            Arc::clone(&self.was_canceled)
        }

        fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError> {
            handle.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn get_regs(&self) -> Result<StandardRegs, HalError> {
            unimplemented!()
        }

        fn set_regs(&mut self, _regs: &StandardRegs) -> Result<(), HalError> {
            unimplemented!()
        }

        fn get_sregs(&self) -> Result<SpecialRegs, HalError> {
            unimplemented!()
        }

        fn set_sregs(&mut self, _sregs: &SpecialRegs) -> Result<(), HalError> {
            unimplemented!()
        }

        fn inject_interrupt(&mut self, _vector: u8) -> Result<(), HalError> {
            unimplemented!()
        }

        fn request_interrupt_window(&mut self) -> Result<(), HalError> {
            unimplemented!()
        }
    }

    #[test]
    fn test_vcpu_cancel() {
        let was_canceled = Arc::new(AtomicBool::new(false));
        let vcpu = DummyVcpu {
            was_canceled: Arc::clone(&was_canceled),
        };
        assert!(vcpu.cancel().is_ok());
        assert!(was_canceled.load(Ordering::SeqCst));
    }
}
