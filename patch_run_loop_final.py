import re

with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    code = f.read()

# Replace .expect("device lock poisoned") with .map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?
code = code.replace(""".expect("device lock poisoned")""", """.map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?""")

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

with open("crates/hitz-vmm/src/run_loop.rs", "w") as f:
    f.write(code)
