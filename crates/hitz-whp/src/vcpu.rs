//! WHP virtual processor — run loop and register access.

use std::sync::Arc;

use hitz_hal::{HalError, SpecialRegs, StandardRegs, Vcpu, VcpuExit, VcpuId};
use windows::Win32::System::Hypervisor::{
    WHV_REGISTER_VALUE, WHV_RUN_VP_EXIT_CONTEXT, WHvCancelRunVirtualProcessor,
    WHvCreateVirtualProcessor, WHvDeleteVirtualProcessor, WHvGetVirtualProcessorRegisters,
    WHvRunVirtualProcessor, WHvSetVirtualProcessorRegisters,
};

use crate::convert;
use crate::partition::PartitionInner;

/// WHP virtual processor implementing [`hitz_hal::Vcpu`].
pub struct WhpVcpu {
    /// Shared partition state (handle, etc.).
    pub(crate) partition: Arc<PartitionInner>,
    /// vCPU index within the partition.
    pub(crate) index: u32,
}

impl WhpVcpu {
    /// Create a new vCPU within the given partition.
    pub(super) fn new(partition: Arc<PartitionInner>, id: VcpuId) -> Result<Self, HalError> {
        let index = id.as_u32();

        // SAFETY: partition handle is valid, index is within bounds (checked by caller).
        unsafe { WHvCreateVirtualProcessor(partition.handle, index, 0) }.map_err(|e| {
            HalError::CreateVcpu {
                vcpu_id: index,
                reason: format!("WHvCreateVirtualProcessor: {e}"),
            }
        })?;

        Ok(Self { partition, index })
    }
}

impl Drop for WhpVcpu {
    fn drop(&mut self) {
        // SAFETY: We own this vCPU index and are tearing it down.
        let _ = unsafe { WHvDeleteVirtualProcessor(self.partition.handle, self.index) };
    }
}

impl Vcpu for WhpVcpu {
    fn run(&mut self) -> Result<VcpuExit, HalError> {
        let mut exit_ctx: WHV_RUN_VP_EXIT_CONTEXT = unsafe { core::mem::zeroed() };

        #[allow(clippy::cast_possible_truncation)] // WHV_RUN_VP_EXIT_CONTEXT is < 4 GiB
        let ctx_size = size_of::<WHV_RUN_VP_EXIT_CONTEXT>() as u32;

        // SAFETY: partition handle is valid, exit_ctx is properly sized.
        unsafe {
            WHvRunVirtualProcessor(
                self.partition.handle,
                self.index,
                (&raw mut exit_ctx).cast(),
                ctx_size,
            )
        }
        .map_err(|e| HalError::VcpuRun(format!("WHvRunVirtualProcessor: {e}")))?;

        convert::exit_context_to_hal(&exit_ctx)
    }

    fn cancel(&self) -> Result<(), HalError> {
        // SAFETY: partition handle is valid, flags must be 0.
        unsafe { WHvCancelRunVirtualProcessor(self.partition.handle, self.index, 0) }
            .map_err(|e| HalError::VcpuCancel(format!("WHvCancelRunVirtualProcessor: {e}")))
    }

    fn get_regs(&self) -> Result<StandardRegs, HalError> {
        let names = convert::STANDARD_REG_NAMES;
        let mut values = [unsafe { core::mem::zeroed::<WHV_REGISTER_VALUE>() }; 18];

        #[allow(clippy::cast_possible_truncation)] // array length is always < u32::MAX
        let count = names.len() as u32;

        // SAFETY: arrays are properly sized and aligned.
        unsafe {
            WHvGetVirtualProcessorRegisters(
                self.partition.handle,
                self.index,
                names.as_ptr(),
                count,
                values.as_mut_ptr(),
            )
        }
        .map_err(|e| HalError::RegisterAccess(format!("get standard regs: {e}")))?;

        // SAFETY: values were just populated by WHvGetVirtualProcessorRegisters
        // with matching STANDARD_REG_NAMES.
        Ok(unsafe { convert::values_to_standard_regs(&values) })
    }

    fn set_regs(&mut self, regs: &StandardRegs) -> Result<(), HalError> {
        let names = convert::STANDARD_REG_NAMES;
        let values = convert::standard_regs_to_values(regs);

        #[allow(clippy::cast_possible_truncation)]
        let count = names.len() as u32;

        // SAFETY: arrays are properly sized and aligned.
        unsafe {
            WHvSetVirtualProcessorRegisters(
                self.partition.handle,
                self.index,
                names.as_ptr(),
                count,
                values.as_ptr(),
            )
        }
        .map_err(|e| HalError::RegisterAccess(format!("set standard regs: {e}")))?;

        Ok(())
    }

    fn get_sregs(&self) -> Result<SpecialRegs, HalError> {
        let names = convert::SPECIAL_REG_NAMES;
        let mut values =
            [unsafe { core::mem::zeroed::<WHV_REGISTER_VALUE>() }; convert::SPECIAL_REG_COUNT];

        #[allow(clippy::cast_possible_truncation)]
        let count = names.len() as u32;

        // SAFETY: arrays are properly sized and aligned.
        unsafe {
            WHvGetVirtualProcessorRegisters(
                self.partition.handle,
                self.index,
                names.as_ptr(),
                count,
                values.as_mut_ptr(),
            )
        }
        .map_err(|e| HalError::RegisterAccess(format!("get special regs: {e}")))?;

        Ok(convert::values_to_special_regs(&values))
    }

    fn inject_interrupt(&mut self, vector: u8) -> Result<(), HalError> {
        use windows::Win32::System::Hypervisor::WHvRegisterPendingInterruption;

        // WHV_X64_PENDING_INTERRUPTION_REGISTER layout:
        //   bit 0: InterruptionPending = 1
        //   bits 1-3: InterruptionType = 0 (external interrupt)
        //   bits 4: DeliverErrorCode = 0
        //   bits 5-8: InstructionLength = 0
        //   bit 9: NestedEvent = 0
        //   bits 10-15: Reserved = 0
        //   bits 16-31: InterruptionVector = vector
        let value = 1u64 | (u64::from(vector) << 16);
        let name = WHvRegisterPendingInterruption;
        let reg_value = convert::reg64_val(value);

        // SAFETY: single register name/value pair, properly aligned.
        unsafe {
            WHvSetVirtualProcessorRegisters(
                self.partition.handle,
                self.index,
                &raw const name,
                1,
                &raw const reg_value,
            )
        }
        .map_err(|e| HalError::InjectInterrupt(format!("WHvSetVirtualProcessorRegisters: {e}")))?;

        Ok(())
    }

    fn request_interrupt_window(&mut self) -> Result<(), HalError> {
        use windows::Win32::System::Hypervisor::WHvX64RegisterDeliverabilityNotifications;

        // Bit 0: InterruptNotification — exit when IF=1 and not in shadow.
        let value = convert::reg64_val(1u64);
        let name = WHvX64RegisterDeliverabilityNotifications;

        // SAFETY: single register name/value pair, properly aligned.
        unsafe {
            WHvSetVirtualProcessorRegisters(
                self.partition.handle,
                self.index,
                &raw const name,
                1,
                &raw const value,
            )
        }
        .map_err(|e| HalError::RegisterAccess(format!("set DeliverabilityNotifications: {e}")))
    }

    fn set_sregs(&mut self, sregs: &SpecialRegs) -> Result<(), HalError> {
        let names = convert::SPECIAL_REG_NAMES;
        let values = convert::special_regs_to_values(sregs);

        // Set registers individually to avoid a WHP batch-call crash
        // (`STATUS_ACCESS_VIOLATION`) that occurs when setting all 14
        // special registers in a single `WHvSetVirtualProcessorRegisters`
        // call with mixed 64-bit + segment + table register types.
        // Setting them one-by-one is functionally identical and reliable.
        for i in 0..names.len() {
            // SAFETY: single register name/value pair, properly aligned.
            unsafe {
                WHvSetVirtualProcessorRegisters(
                    self.partition.handle,
                    self.index,
                    &raw const names[i],
                    1,
                    &raw const values[i],
                )
            }
            .map_err(|e| {
                HalError::RegisterAccess(format!("set special reg {}: {e}", names[i].0))
            })?;
        }

        Ok(())
    }
}
