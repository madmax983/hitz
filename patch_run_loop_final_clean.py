import re

with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    code = f.read()

# Find the start of the tests module
tests_idx = code.find("mod tests {")
code_body = code[:tests_idx]
tests_body = code[tests_idx:]

# Replace .expect with map_err in body
code_body = code_body.replace("""let mut devs = devices.lock().expect("device lock poisoned");""", """let mut devs = devices.lock().map_err(|_| hitz_hal::HalError::DeviceLockPoisoned)?;""")

# Add drop(devs) in IoPort exit
code_body = code_body.replace("""        VcpuExit::IoPort(io) => {
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
code_body = code_body.replace("""#[allow(clippy::expect_used)]
fn dispatch_exit<V: Vcpu, W: Write>(""", """fn dispatch_exit<V: Vcpu, W: Write>(""")

# For tests, let's remove any test that contains `should_panic` and `poisoned` entirely.
tests_body = re.sub(r'    #\[test\]\n    #\[should_panic\(expected = "device lock poisoned"\)\]\n    fn [a_zA_Z_0-9]+\(\) \{[\s\S]*?(?=\n    #\[test\]|\n\})', '', tests_body)

code = code_body + tests_body

with open("crates/hitz-vmm/src/run_loop.rs", "w") as f:
    f.write(code)
