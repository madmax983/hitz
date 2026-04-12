#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use hitz_devices::{MmioBus, SerialDevice};
use hitz_hal::GuestMemAccess;
use hitz_hal::{Gpa, HalError, MmioExit, SpecialRegs, StandardRegs, Vcpu, VcpuExit};
use hitz_vmm::run_loop::{SharedDevices, run_vcpu_loop};
use proptest::prelude::*;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

struct MaliciousVcpu {
    exit_to_return: VcpuExit,
    run_called: bool,
}

impl Vcpu for MaliciousVcpu {
    type CancelHandle = ();

    fn run(&mut self) -> Result<VcpuExit, HalError> {
        if self.run_called {
            return Ok(VcpuExit::Halt);
        }
        self.run_called = true;

        let exit = match &self.exit_to_return {
            VcpuExit::Mmio(m) => VcpuExit::Mmio(MmioExit { ..*m }),
            _ => VcpuExit::Halt,
        };

        Ok(exit)
    }

    fn cancel_handle(&self) -> Self::CancelHandle {}
    fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), HalError> {
        Ok(())
    }
    fn get_regs(&self) -> Result<StandardRegs, HalError> {
        Ok(StandardRegs::default())
    }
    fn set_regs(&mut self, _: &StandardRegs) -> Result<(), HalError> {
        Ok(())
    }
    fn get_sregs(&self) -> Result<SpecialRegs, HalError> {
        Ok(SpecialRegs::default())
    }
    fn set_sregs(&mut self, _: &SpecialRegs) -> Result<(), HalError> {
        Ok(())
    }
    fn request_interrupt_window(&mut self) -> Result<(), HalError> {
        Ok(())
    }
    fn inject_interrupt(&mut self, _: u8) -> Result<(), HalError> {
        Ok(())
    }
    fn cancel(&self) -> Result<(), HalError> {
        Ok(())
    }
}

struct DummyMem;
impl GuestMemAccess for DummyMem {
    fn read_guest(&self, _: u64, _: &mut [u8]) -> Result<(), HalError> {
        Ok(())
    }
    fn write_guest(&self, _: u64, _: &[u8]) -> Result<(), HalError> {
        Ok(())
    }
}

proptest! {
    #[test]
    fn torture_handle_mmio_out_of_bounds_byte_count(
        byte_count in 17..=255u8, // > 16 causes panic when slicing mmio.instruction_bytes
    ) {
        let mut vcpu = MaliciousVcpu {
            exit_to_return: VcpuExit::Mmio(MmioExit {
                gpa: Gpa::new(0),
                data: [0; 8],
                len: 4,
                is_write: true,
                instruction_len: 0,
                instruction_bytes: [0; 16],
                instruction_byte_count: byte_count,
            }),
            run_called: false,
        };

        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        let mem = DummyMem;
        let stop_flag = AtomicBool::new(false);

        let _ = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);
    }
}
