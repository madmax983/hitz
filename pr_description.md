💡 What: Removed an unnecessary `.to_string()` heap allocation in `hitz-vmm/src/vm.rs` inside an `std::io::Error` instantiation on the vCPU error path.
🎯 Why: `std::io::Error::new()` accepts any type that implements `Into<Box<dyn std::error::Error + Send + Sync>>`, which includes `&str`. Converting a static string slice to an owned `String` via `.to_string()` incurs an unneeded heap allocation and memory copy, violating the zero-cost abstraction philosophy.
📊 Impact: Minor reduction in heap allocations when handling a vCPU creation failure path.
🔬 Measurement: Verified with `cargo clippy -p hitz-vmm`.
