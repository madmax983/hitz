with open('./crates/hitz-vmm/src/run_loop.rs', 'r') as f:
    content = f.read()

test_to_add = """
    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_poll_devices_with_poisoned_lock() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        let _ = std::panic::catch_unwind(|| {
            let _guard = devices.lock().unwrap();
            panic!("poisoning");
        });

        let mut pending = None;
        let _ = poll_devices(&mut vcpu, &devices, &mut pending);
    }

    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_ioport_with_poisoned_lock() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        let _ = std::panic::catch_unwind(|| {
            let _guard = devices.lock().unwrap();
            panic!("poisoning");
        });

        let mut pending = None;
        let counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let io = hitz_hal::IoPortExit {
            port: 0x3F8,
            data: [b'A', 0, 0, 0],
            len: 1,
            is_write: true,
            instruction_len: 2,
        };

        let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::IoPort(io),
            &mut pending,
            &counter,
        );
    }

    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_mmio_read_with_poisoned_lock() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        let _ = std::panic::catch_unwind(|| {
            let _guard = devices.lock().unwrap();
            panic!("poisoning");
        });

        let mut pending = None;
        let counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let mmio = hitz_hal::MmioExit {
            gpa: 0x1000,
            data: [0; 8],
            len: 4,
            is_write: false,
            // 8B 05 00 00 00 00 (MOV eax, [rip+disp32])
            instruction_bytes: [
                0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            instruction_byte_count: 6,
            instruction_len: 6,
        };

        let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );
    }

    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_mmio_write_with_poisoned_lock() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        let _ = std::panic::catch_unwind(|| {
            let _guard = devices.lock().unwrap();
            panic!("poisoning");
        });

        let mut pending = None;
        let counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

        let mmio = hitz_hal::MmioExit {
            gpa: 0x1000,
            data: [0; 8],
            len: 4,
            is_write: true,
            // C7 05 00 00 00 00 12 34 56 78 (MOV [rip+disp32], imm32)
            instruction_bytes: [
                0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0, 0, 0,
            ],
            instruction_byte_count: 10,
            instruction_len: 10,
        };

        let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );
    }
}
"""

if content.endswith("}\n"):
    new_content = content[:-2] + test_to_add
    with open('./crates/hitz-vmm/src/run_loop.rs', 'w') as f:
        f.write(new_content)
    print("Patched successfully")
else:
    print("Could not find end of file")
