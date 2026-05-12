import re

with open("crates/hitz-vmm/src/vm.rs", "r") as f:
    code = f.read()

# Fix the shared_handles collect
code = code.replace(".collect::<Result<Vec<_>, _>>()?;", ".collect();")
code = code.replace("let shared_handles: Arc<[_]> = vcpus.iter().map(Vcpu::cancel_handle).collect::<Result<Vec<_>, _>>()?;", "let shared_handles: Arc<[_]> = vcpus.iter().map(Vcpu::cancel_handle).collect();")

# Fix `watchdog?.join()` back to `.join().unwrap()` or similar since we map_err instead? No wait!
# `watchdog` is returned from `.spawn().map_err(...)`.
# Wait, `std::thread::Builder::spawn` returns `Result<JoinHandle, io::Error>`.
# If I use `.map_err(...)` inside the outer block but NO `?`, then `watchdog` is `Result<JoinHandle, VmError>`.
# If I use `.map_err(...)` AND `?`, `watchdog` is `JoinHandle`, and the error is returned!
# I used `.map_err(...)` on `watchdog` in `run_single_vcpu`?
# In `run_single_vcpu`:
# `let watchdog = std::thread::Builder::new().spawn(...).map_err(...)?;`
code = code.replace(".map_err(|e| VmError::ThreadSpawn(e.to_string()))\n    };", ".map_err(|e| VmError::ThreadSpawn(e.to_string()))?\n    };")
code = code.replace(".map_err(|e| VmError::ThreadSpawn(e.to_string()))\n        })", ".map_err(|e| VmError::ThreadSpawn(e.to_string()))\n        })")
# Let's just fix it properly with regex.

# Revert the changes to vm.rs to the state right before we started patching it.
