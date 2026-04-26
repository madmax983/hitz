with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    content = f.read()

tests = """
    #[test]
    fn test_dispatch_exit_unknown() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Unknown(0x1337),
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(Vec::new()),
            mmio_bus: MmioBus::new(),
        });
        let mut pending = None;
        let counter = opentelemetry::global::meter("test").u64_counter("test").build();

        let reason = dispatch_exit(&mut vcpu, &devices, &DummyMem, VcpuExit::Unknown(0x1337), &mut pending, &counter).unwrap();
        if let Some(ExitReason::Unexpected(msg)) = reason {
            assert!(msg.contains("1337"));
        } else {
            panic!("Expected Unexpected reason");
        }
    }

    #[test]
    fn test_dispatch_exit_canceled() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Canceled,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(Vec::new()),
            mmio_bus: MmioBus::new(),
        });
        let mut pending = Some(42);
        let counter = opentelemetry::global::meter("test").u64_counter("test").build();

        let reason = dispatch_exit(&mut vcpu, &devices, &DummyMem, VcpuExit::Canceled, &mut pending, &counter).unwrap();
        assert!(reason.is_none());
        assert_eq!(pending, Some(42));
        assert!(vcpu.regs.rip == 0); // Vcpu.request_interrupt_window doesn't change rip, but it was called
    }

    #[test]
    fn test_dispatch_exit_interrupt_window() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::InterruptWindow,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(Vec::new()),
            mmio_bus: MmioBus::new(),
        });
        let mut pending = Some(42);
        let counter = opentelemetry::global::meter("test").u64_counter("test").build();

        let reason = dispatch_exit(&mut vcpu, &devices, &DummyMem, VcpuExit::InterruptWindow, &mut pending, &counter).unwrap();
        assert!(reason.is_none());
        assert_eq!(pending, None); // Should be consumed
    }
"""

content = content.replace("    #[test]\n    fn test_dispatch_exit_halt()", tests + "\n    #[test]\n    fn test_dispatch_exit_halt()")

with open("crates/hitz-vmm/src/run_loop.rs", "w") as f:
    f.write(content)
