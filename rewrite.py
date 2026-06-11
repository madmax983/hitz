import re

with open('crates/hitz-vmm/src/vm.rs', 'r') as f:
    content = f.read()

pattern2 = re.compile(r'let handles: Vec<_> = vcpus\n        \.into_iter\(\)\n        \.enumerate\(\)\n        \.map\(\\|\(idx, mut vcpu\)\\| \{\n(.*?)        \}\)\n        \.collect\(\);', re.DOTALL)

def replacement(match):
    inner = match.group(1)
    return f"""// ⚡ Bolt Optimization: Pre-allocate `handles` vector to avoid reallocation during VM boot.
    let mut handles = Vec::with_capacity(num_vcpus);
    for (idx, mut vcpu) in vcpus.into_iter().enumerate() {{
{inner}        handles.push(
            std::thread::Builder::new()
                .name(format!("vcpu-{{idx}}"))
                .spawn(move || {{
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{
                        run_loop::run_vcpu_loop(&mut vcpu, &devs, &*mem, &stop)
                    }}));

                    match &result {{
                        Ok(
                            Ok(ExitReason::Halt | ExitReason::Shutdown | ExitReason::Unexpected(_))
                            | Err(_),
                        )
                        | Err(_) => {{
                            stop.store(true, Ordering::Relaxed);
                            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&cancel_handles);
                        }}
                        Ok(Ok(ExitReason::Canceled)) => {{}}
                    }}

                    let exit = match result {{
                        Ok(r) => r,
                        Err(payload) => {{
                            let msg = payload.downcast_ref::<&str>().map_or_else(
                                || {{
                                    payload
                                        .downcast_ref::<String>()
                                        .map_or_else(|| "unknown panic".to_string(), Clone::clone)
                                }},
                                |s| (*s).to_string(),
                            );
                            Ok(ExitReason::Unexpected(format!(
                                "vCPU {{idx}} panicked: {{msg}}"
                            )))
                        }}
                    }};

                    let _ = tx.send(exit);
                }})
                .expect("spawn vcpu thread"),
        );
    }}"""

new_content = pattern2.sub(replacement, content)

with open('crates/hitz-vmm/src/vm.rs', 'w') as f:
    f.write(new_content)
