import re

with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    code = f.read()

# Only replace the ones in the functions, not tests.
# `let mut devs = devices.lock().expect("device lock poisoned");`
code = code.replace("""let mut devs = devices.lock().expect("device lock poisoned");""", """let mut devs = devices.lock().map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?;""")

# Add drop(devs) in IoPort exit
code = code.replace("""        VcpuExit::IoPort(io) => {
            record_exit(exit_counter, "IoPort");
            let mut devs = devices.lock().map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?;
            handle_io_port(vcpu, &mut devs.serial, &io)?;
            Ok(None)
        }""", """        VcpuExit::IoPort(io) => {
            record_exit(exit_counter, "IoPort");
            let mut devs = devices.lock().map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?;
            handle_io_port(vcpu, &mut devs.serial, &io)?;
            drop(devs);
            Ok(None)
        }""")

# Remove #[allow(clippy::expect_used)] on dispatch_exit
code = code.replace("""#[allow(clippy::expect_used)]
fn dispatch_exit<V: Vcpu, W: Write>(""", """fn dispatch_exit<V: Vcpu, W: Write>(""")

# For the tests: rename `should_panic_on_...` to `should_return_err_on_...` and change `#[should_panic]` to asserting error.
code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_mmio_read_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_dispatch_exit_mmio_read_with_poisoned_lock() {""")
code = code.replace("""let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );""", """let res = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );
        assert!(matches!(res, Err(hitz_hal::HalError::DeviceLockPoisoned)));""")

code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_mmio_write_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_dispatch_exit_mmio_write_with_poisoned_lock() {""")
code = code.replace("""let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );""", """let res = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Mmio(mmio),
            &mut pending,
            &counter,
        );
        assert!(matches!(res, Err(hitz_hal::HalError::DeviceLockPoisoned)));""")

code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_poll_devices_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_poll_devices_with_poisoned_lock() {""")
code = code.replace("""let _ = poll_devices(&mut vcpu, &devices, &mut pending);""", """let res = poll_devices(&mut vcpu, &devices, &mut pending);
        assert!(matches!(res, Err(hitz_hal::HalError::DeviceLockPoisoned)));""")

code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_ioport_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_dispatch_exit_ioport_with_poisoned_lock() {""")
code = code.replace("""let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::IoPort(io),
            &mut pending,
            &counter,
        );""", """let res = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::IoPort(io),
            &mut pending,
            &counter,
        );
        assert!(matches!(res, Err(hitz_hal::HalError::DeviceLockPoisoned)));""")

code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_run_vcpu_loop_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_run_vcpu_loop_with_poisoned_lock() {""")
code = code.replace("""let _ = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);""", """let res = run_vcpu_loop(&mut vcpu, &devices, &mem, &stop_flag);
        assert!(matches!(res, Err(VmError::Hal(hitz_hal::HalError::DeviceLockPoisoned))));""")

code = code.replace("""    #[test]
    #[should_panic(expected = "device lock poisoned")]
    fn should_panic_on_dispatch_exit_canceled_with_poisoned_lock() {""", """    #[test]
    fn should_return_err_on_dispatch_exit_canceled_with_poisoned_lock() {""")
code = code.replace("""let _ = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Canceled,
            &mut pending,
            &counter,
        );""", """let res = dispatch_exit(
            &mut vcpu,
            &devices,
            &DummyMem,
            VcpuExit::Canceled,
            &mut pending,
            &counter,
        );
        assert!(matches!(res, Err(hitz_hal::HalError::DeviceLockPoisoned)));""")


with open("crates/hitz-vmm/src/run_loop.rs", "w") as f:
    f.write(code)
