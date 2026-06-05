use hitz_vmm::run_loop::{run_vcpu_loop, SharedDevices};
use hitz_hal::{Gpa, MmioExit, StandardRegs, Vcpu, VcpuExit, HalError, GuestMemAccess};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

struct DummyVcpu {
    rip: u64,
    exits: Vec<VcpuExit>,
}

impl Vcpu for DummyVcpu {
    fn run(&mut self) -> Result<VcpuExit, HalError> {
        if self.exits.is_empty() {
            return Ok(VcpuExit::Halt);
        }
        Ok(self.exits.remove(0))
    }

    fn get_regs(&self) -> Result<StandardRegs, HalError> {
        let mut regs = StandardRegs::default();
        regs.rip = self.rip;
        Ok(regs)
    }

    fn set_regs(&mut self, regs: &StandardRegs) -> Result<(), HalError> {
        self.rip = regs.rip;
        Ok(())
    }

    fn inject_interrupt(&mut self, _vector: u8) -> Result<(), HalError> { Ok(()) }
    fn request_interrupt_window(&mut self) -> Result<(), HalError> { Ok(()) }
    fn id(&self) -> hitz_hal::VcpuId { hitz_hal::VcpuId::new(0) }
    fn cancel_handle(&self) -> Box<dyn hitz_hal::VcpuCancel> { unimplemented!() }
}

struct DummyMem;
impl GuestMemAccess for DummyMem {
    fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), HalError> { Ok(()) }
    fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), HalError> { Ok(()) }
}

#[test]
fn havoc_test_mmio_infinite_loop() {
    // 👺 We inject an undecodable MMIO exit (0x00 opcode)
    // WHP will report instruction_len = 0.
    // The VMM should NOT loop infinitely. If it does, we timeout.
    let mut vcpu = DummyVcpu {
        rip: 0x1000,
        exits: vec![
            VcpuExit::Mmio(MmioExit {
                gpa: Gpa::new(0xD000_0000),
                is_write: true,
                instruction_bytes: [0x00; 16], // Invalid/undecodable instruction
                instruction_byte_count: 1,
                instruction_len: 0,            // WHP sets this to 0 for MMIO
                data: [0; 8],
                len: 0,
            }),
            VcpuExit::Halt,
        ],
    };

    let devices = Mutex::new(SharedDevices {
        serial: hitz_devices::SerialDevice::new(std::io::sink()),
        mmio_bus: hitz_devices::MmioBus::new(),
    });
    let stop_flag = AtomicBool::new(false);
    let mem = DummyMem;

    let res = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("undecodable MMIO instruction at GPA"));
}
