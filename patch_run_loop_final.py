import sys

def modify_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # The tests were removed earlier because cargo test failed to compile them
    # Let's add them back correctly under the tests module.

    replace_str = """    #[test]
    fn test_dispatch_exit_shutdown() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let mut pending_irq = None;
        let exit_counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let result = dispatch_exit(&mut vcpu, &devices, &mem, VcpuExit::Shutdown, &mut pending_irq, &exit_counter)
            .expect("dispatch_exit should succeed");

        assert_eq!(result, Some(ExitReason::Shutdown));
    }

    #[test]
    fn test_dispatch_exit_interrupt_window() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let mut pending_irq = Some(42);
        let exit_counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let result = dispatch_exit(&mut vcpu, &devices, &mem, VcpuExit::InterruptWindow, &mut pending_irq, &exit_counter)
            .expect("dispatch_exit should succeed");

        assert_eq!(result, None);
        assert_eq!(pending_irq, None); // Should have been taken and injected
    }

    #[test]
    fn test_dispatch_exit_mmio() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });
        let mem = DummyMem;
        let mut pending_irq = None;
        let exit_counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let mmio = hitz_hal::MmioExit {
            gpa: hitz_hal::Gpa::new(0x1000),
            data: [0; 8],
            len: 4,
            is_write: false,
            instruction_bytes: [0; 16],
            instruction_byte_count: 2,
        };

        let result = dispatch_exit(&mut vcpu, &devices, &mem, VcpuExit::Mmio(mmio), &mut pending_irq, &exit_counter)
            .expect("dispatch_exit should succeed");

        assert_eq!(result, None);
        assert_eq!(vcpu.regs.rip, 2); // RIP should advance
    }

    #[test]
    fn test_run_vcpu_loop_halt() {"""

    search_str = """    #[test]
    fn test_run_vcpu_loop_halt() {"""

    with open(filepath, 'w') as f:
        f.write(content.replace(search_str, replace_str))

modify_file('crates/hitz-vmm/src/run_loop.rs')

def modify_file2(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    replace_str = """pub(crate) mod run_loop;"""
    search_str = """pub mod run_loop;"""

    with open(filepath, 'w') as f:
        f.write(content.replace(search_str, replace_str))

modify_file2('crates/hitz-vmm/src/lib.rs')
