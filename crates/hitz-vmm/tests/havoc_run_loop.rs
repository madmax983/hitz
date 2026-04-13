use hitz_devices::{MmioBus, SerialDevice};
use hitz_hal::{HalError, MmioExit, SpecialRegs, StandardRegs, Vcpu, VcpuExit};
use hitz_vmm::run_loop::*;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

struct DummyMem;
impl hitz_hal::GuestMemAccess for DummyMem {
    fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), HalError> {
        Ok(())
    }
    fn write_guest(&self, _gpa: u64, _buf: &[u8]) -> Result<(), HalError> {
        Ok(())
    }
}

struct DummyVcpu {
    exit: VcpuExit,
}

impl Vcpu for DummyVcpu {
    type CancelHandle = ();
    fn cancel_handle(&self) -> Self::CancelHandle {}
    fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), HalError> {
        Ok(())
    }

    fn run(&mut self) -> Result<VcpuExit, HalError> {
        Ok(self.exit.clone())
    }
    fn get_regs(&self) -> Result<StandardRegs, HalError> {
        Ok(StandardRegs::default())
    }
    fn set_regs(&mut self, _regs: &StandardRegs) -> Result<(), HalError> {
        Ok(())
    }
    fn get_sregs(&self) -> Result<SpecialRegs, HalError> {
        Ok(SpecialRegs::default())
    }
    fn set_sregs(&mut self, _sregs: &SpecialRegs) -> Result<(), HalError> {
        Ok(())
    }
    fn inject_interrupt(&mut self, _vector: u8) -> Result<(), HalError> {
        Ok(())
    }
    fn request_interrupt_window(&mut self) -> Result<(), HalError> {
        Ok(())
    }
}

#[test]
fn havoc_mmio_out_of_bounds() {
    let mut vcpu = DummyVcpu {
        exit: VcpuExit::Mmio(MmioExit {
            gpa: hitz_hal::Gpa::new(0x1000),
            is_write: true,
            data: [0; 8],
            len: 0,
            instruction_bytes: [0; 16],
            instruction_byte_count: 20, // GREATER THAN 16
            instruction_len: 2,
        }),
    };

    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(std::io::sink()),
        mmio_bus: MmioBus::new(),
    });

    let stop_flag = AtomicBool::new(false);
    let mem = DummyMem;

    // This will panic if we slice using `..usize::from(mmio.instruction_byte_count)`
    // because instruction_bytes is only 16 bytes.
    let _ = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);
}
