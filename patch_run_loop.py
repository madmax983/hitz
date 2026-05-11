with open('crates/hitz-vmm/src/run_loop.rs', 'r') as f:
    content = f.read()

# Insert the test inside `mod tests {` at the end
# The end of the file is:
#     #[test]
#     #[should_panic(expected = "device lock poisoned")]
#     fn should_panic_on_dispatch_exit_canceled_with_poisoned_lock() {
#        ...
#     }
# }

new_test = """
    #[test]
    #[should_panic]
    fn havoc_test_mmio_read_buffer_overflow() {
        let mut vcpu = DummyVcpu {
            regs: Default::default(),
            sregs: Default::default(),
            exit: VcpuExit::Halt,
        };
        let devices = Mutex::new(SharedDevices {
            serial: SerialDevice::new(std::io::sink()),
            mmio_bus: MmioBus::new(),
        });

        // Simulate an MMIO read with size 12
        let mmio = hitz_hal::MmioExit {
            gpa: hitz_hal::Gpa::new(0x1000),
            data: [0; 8],
            len: 12,
            is_write: false,
            instruction_bytes: [
                0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            instruction_byte_count: 6,
            instruction_len: 6,
        };

        let mut pending = None;
        let counter = opentelemetry::global::meter("hitz")
            .u64_counter("hitz.vcpu.exits")
            .build();

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

content = content.rsplit('}\n', 1)[0] + new_test

# Also need to fix gpa: 0x1000 -> gpa: hitz_hal::Gpa::new(0x1000) for all MmioExits
content = content.replace('gpa: 0x1000', 'gpa: hitz_hal::Gpa::new(0x1000)')

with open('crates/hitz-vmm/src/run_loop.rs', 'w') as f:
    f.write(content)
