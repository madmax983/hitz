//! WHP partition — owns a `WHV_PARTITION_HANDLE`.

use std::sync::Arc;

use hitz_hal::{Gpa, HalError, MemFlags, Partition, PartitionConfig, VcpuId};
use windows::Win32::System::Hypervisor::{
    WHV_MAP_GPA_RANGE_FLAGS, WHV_PARTITION_HANDLE, WHV_PARTITION_PROPERTY, WHvCreatePartition,
    WHvDeletePartition, WHvMapGpaRange, WHvPartitionPropertyCodeLocalApicEmulationMode,
    WHvPartitionPropertyCodeProcessorCount, WHvSetPartitionProperty, WHvSetupPartition,
    WHvUnmapGpaRange, WHvX64LocalApicEmulationModeXApic,
};

use crate::vcpu::WhpVcpu;

/// Shared state for a WHP partition, accessible by all vCPUs.
pub(crate) struct PartitionInner {
    /// The WHP partition handle.
    pub(crate) handle: WHV_PARTITION_HANDLE,
}

// SAFETY: WHV_PARTITION_HANDLE is a process-wide handle that can be used
// from any thread. WHP APIs are documented as thread-safe per-partition.
unsafe impl Send for PartitionInner {}
unsafe impl Sync for PartitionInner {}

impl Drop for PartitionInner {
    fn drop(&mut self) {
        // SAFETY: We own the partition handle and are tearing it down.
        let _ = unsafe { WHvDeletePartition(self.handle) };
    }
}

/// WHP partition implementing [`hitz_hal::Partition`].
pub struct WhpPartition {
    pub(crate) inner: Arc<PartitionInner>,
    vcpu_count: u32,
}

impl WhpPartition {
    /// Create and set up a new WHP partition.
    pub(super) fn new(cfg: &PartitionConfig) -> Result<Self, HalError> {
        // Step 1: Create the partition object.
        let handle = unsafe { WHvCreatePartition() }
            .map_err(|e| HalError::CreatePartition(format!("WHvCreatePartition: {e}")))?;

        // Step 2: Set processor count before setup.
        let prop = WHV_PARTITION_PROPERTY {
            ProcessorCount: cfg.vcpu_count,
        };

        #[allow(clippy::cast_possible_truncation)] // WHV_PARTITION_PROPERTY is < 4 GiB
        let prop_size = size_of::<WHV_PARTITION_PROPERTY>() as u32;

        unsafe {
            WHvSetPartitionProperty(
                handle,
                WHvPartitionPropertyCodeProcessorCount,
                (&raw const prop).cast(),
                prop_size,
            )
        }
        .map_err(|e| HalError::SetupPartition(format!("set ProcessorCount: {e}")))?;

        // Step 2b: Enable WHP built-in xAPIC emulation.
        // This gives us LAPIC timer, interrupt delivery, and EOI — required
        // for Linux boot (setup_local_APIC). Harmless for simple real-mode stubs.
        let apic_prop = WHV_PARTITION_PROPERTY {
            LocalApicEmulationMode: WHvX64LocalApicEmulationModeXApic,
        };
        unsafe {
            WHvSetPartitionProperty(
                handle,
                WHvPartitionPropertyCodeLocalApicEmulationMode,
                (&raw const apic_prop).cast(),
                prop_size,
            )
        }
        .map_err(|e| HalError::SetupPartition(format!("set LocalApicEmulationMode: {e}")))?;

        // Step 3: Finalize the partition — properties are frozen after this.
        unsafe { WHvSetupPartition(handle) }
            .map_err(|e| HalError::SetupPartition(format!("WHvSetupPartition: {e}")))?;

        Ok(Self {
            inner: Arc::new(PartitionInner { handle }),
            vcpu_count: cfg.vcpu_count,
        })
    }
}

impl Partition for WhpPartition {
    type Vcpu = WhpVcpu;

    unsafe fn map_memory(
        &mut self,
        gpa: Gpa,
        hva: *mut u8,
        size: usize,
        flags: MemFlags,
    ) -> Result<(), HalError> {
        let whp_flags = mem_flags_to_whp(flags);

        // SAFETY: Caller guarantees hva points to valid memory of `size` bytes.
        unsafe {
            WHvMapGpaRange(
                self.inner.handle,
                hva.cast(),
                gpa.as_u64(),
                size as u64,
                whp_flags,
            )
        }
        .map_err(|e| HalError::MapMemory {
            gpa: gpa.as_u64(),
            size,
            reason: format!("WHvMapGpaRange: {e}"),
        })
    }

    fn unmap_memory(&mut self, gpa: Gpa, size: usize) -> Result<(), HalError> {
        unsafe { WHvUnmapGpaRange(self.inner.handle, gpa.as_u64(), size as u64) }.map_err(|e| {
            HalError::UnmapMemory {
                gpa: gpa.as_u64(),
                reason: format!("WHvUnmapGpaRange: {e}"),
            }
        })
    }

    fn create_vcpu(&mut self, id: VcpuId) -> Result<Self::Vcpu, HalError> {
        if id.as_u32() >= self.vcpu_count {
            return Err(HalError::CreateVcpu {
                vcpu_id: id.as_u32(),
                reason: format!(
                    "vCPU index {} exceeds partition limit {}",
                    id.as_u32(),
                    self.vcpu_count
                ),
            });
        }
        WhpVcpu::new(Arc::clone(&self.inner), id)
    }

    fn request_interrupt(&self, vcpu_id: VcpuId, _vector: u8) -> Result<(), HalError> {
        // Cancel the vCPU so it exits the run loop. The vCPU thread will
        // then check for pending interrupts and use the interrupt window
        // mechanism to inject them. Full implementation in Phase 3.
        use windows::Win32::System::Hypervisor::WHvCancelRunVirtualProcessor;
        unsafe { WHvCancelRunVirtualProcessor(self.inner.handle, vcpu_id.as_u32(), 0) }
            .map_err(|e| HalError::InterruptRequest(format!("WHvCancelRunVirtualProcessor: {e}")))
    }
}

/// Convert HAL [`MemFlags`] to WHP map flags.
const fn mem_flags_to_whp(flags: MemFlags) -> WHV_MAP_GPA_RANGE_FLAGS {
    let mut bits = 0i32;
    if flags.read {
        bits |= 1; // WHvMapGpaRangeFlagRead
    }
    if flags.write {
        bits |= 2; // WHvMapGpaRangeFlagWrite
    }
    if flags.execute {
        bits |= 4; // WHvMapGpaRangeFlagExecute
    }
    WHV_MAP_GPA_RANGE_FLAGS(bits)
}
