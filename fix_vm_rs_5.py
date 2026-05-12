import re

with open("crates/hitz-vmm/src/vm.rs", "r") as f:
    code = f.read()

# Fix the shared_handles collect
code = code.replace(".collect::<Result<Vec<_>, VmError>>()?;", ".collect();")
code = code.replace("let shared_handles: Arc<[_]> = vcpus.iter().map(Vcpu::cancel_handle).collect::<Result<Vec<_>, VmError>>()?;", "let shared_handles: Arc<[_]> = vcpus.iter().map(Vcpu::cancel_handle).collect();")

# Remove ? from map_err
code = code.replace(""".map_err(|e| VmError::ThreadSpawn(e.to_string()))?
        })""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))
        })""")

# Because the map closure returns Result, we need to collect it into a Result
code = code.replace("""let handles: Vec<_> = vcpus
        .into_iter()
        .enumerate()
        .map(|(idx, mut vcpu)| {
            let devs = Arc::clone(devices);
            let mem = Arc::clone(guest_mem_arc);
            let stop = Arc::clone(stop_flag);
            let cancel_handles = Arc::clone(&shared_handles);
            let tx = exit_tx.clone();

            std::thread::Builder::new()
                .name(format!("vcpu-{idx}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_loop::run_vcpu_loop(&mut vcpu, &devs, &*mem, &stop)
                    }));

                    match &result {
                        Ok(
                            Ok(ExitReason::Halt | ExitReason::Shutdown | ExitReason::Unexpected(_))
                            | Err(_),
                        ) => {
                            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&cancel_handles);
                        }
                        _ => {} // Canceled exits shouldn't cascade cancels.
                    }

                    let _ = tx.send(
                        result.unwrap_or_else(|_| Err(hitz_hal::HalError::VcpuRun("panic".into()))),
                    );
                })
                .map_err(|e| VmError::ThreadSpawn(e.to_string()))
        })
        .collect();""", """let handles: Vec<_> = vcpus
        .into_iter()
        .enumerate()
        .map(|(idx, mut vcpu)| {
            let devs = Arc::clone(devices);
            let mem = Arc::clone(guest_mem_arc);
            let stop = Arc::clone(stop_flag);
            let cancel_handles = Arc::clone(&shared_handles);
            let tx = exit_tx.clone();

            std::thread::Builder::new()
                .name(format!("vcpu-{idx}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_loop::run_vcpu_loop(&mut vcpu, &devs, &*mem, &stop)
                    }));

                    match &result {
                        Ok(
                            Ok(ExitReason::Halt | ExitReason::Shutdown | ExitReason::Unexpected(_))
                            | Err(_),
                        ) => {
                            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&cancel_handles);
                        }
                        _ => {} // Canceled exits shouldn't cascade cancels.
                    }

                    let _ = tx.send(
                        result.unwrap_or_else(|_| Err(hitz_hal::HalError::VcpuRun("panic".into()))),
                    );
                })
                .map_err(|e| VmError::ThreadSpawn(e.to_string()))
        })
        .collect::<Result<Vec<_>, VmError>>()?;""")

with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(code)
