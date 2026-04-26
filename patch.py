import re

with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    content = f.read()

tests = """
    #[test]
    fn test_handle_io_port_serial_write() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x3F8,
            is_write: true,
            instruction_len: 2,
            data: [b'A', 0, 0, 0],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        assert_eq!(serial.writer(), b"A");
    }

    #[test]
    fn test_handle_io_port_serial_read() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 0xFFFF_FFFF,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x3FD, // LSR
            is_write: false,
            instruction_len: 2,
            data: [0; 4],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        // LSR should have THRE (0x20) and TEMT (0x40) set
        assert_eq!(vcpu.regs.rax & 0xFF, 0x60);
    }

    #[test]
    fn test_handle_io_port_pic_write() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x20, // PIC
            is_write: true,
            instruction_len: 2,
            data: [0x11, 0, 0, 0],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 102);
    }

    #[test]
    fn test_handle_io_port_pic_read() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 0xFFFF_FFFF,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x21, // PIC
            is_write: false,
            instruction_len: 2,
            data: [0; 4],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 102);
        assert_eq!(vcpu.regs.rax, 0xFFFF_FF00); // Only lowest byte is replaced with 0x00
    }

    #[test]
    fn test_handle_io_port_unhandled_read() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                rax: 0xFFFF_FFFF_FFFF_0000,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x1234, // Unknown
            is_write: false,
            instruction_len: 3,
            data: [0; 4],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 103);
        assert_eq!(vcpu.regs.rax, 0xFFFF_FFFF_FFFF_00FF); // Returns 0xFF
    }

    #[test]
    fn test_handle_io_port_unhandled_write() {
        let mut vcpu = DummyVcpu {
            regs: hitz_hal::StandardRegs {
                rip: 100,
                ..Default::default()
            },
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let mut serial = SerialDevice::new(Vec::new());

        let io = hitz_hal::IoPortExit {
            port: 0x1234, // Unknown
            is_write: true,
            instruction_len: 3,
            data: [0x42, 0, 0, 0],
        };

        handle_io_port(&mut vcpu, &mut serial, &io).expect("should succeed");
        assert_eq!(vcpu.regs.rip, 103);
    }

    #[test]
    fn test_dispatch_exit_halt() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(Vec::new()),
            mmio_bus: MmioBus::new(),
        });
        let mut pending = None;
        let counter = opentelemetry::global::meter("test").u64_counter("test").build();

        let reason = dispatch_exit(&mut vcpu, &devices, &DummyMem, VcpuExit::Halt, &mut pending, &counter).unwrap();
        assert!(matches!(reason, Some(ExitReason::Halt)));
    }

    #[test]
    fn test_dispatch_exit_shutdown() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Shutdown,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(Vec::new()),
            mmio_bus: MmioBus::new(),
        });
        let mut pending = None;
        let counter = opentelemetry::global::meter("test").u64_counter("test").build();

        let reason = dispatch_exit(&mut vcpu, &devices, &DummyMem, VcpuExit::Shutdown, &mut pending, &counter).unwrap();
        assert!(matches!(reason, Some(ExitReason::Shutdown)));
    }
"""

content = content.replace("    struct DummyVcpu {", tests + "\n    struct DummyVcpu {")

with open("crates/hitz-vmm/src/run_loop.rs", "w") as f:
    f.write(content)
