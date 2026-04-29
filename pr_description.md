💡 What: Replaced `Vec::new()` with `Vec::with_capacity(2)` in `hitz-api/src/rightsizer.rs`.
🎯 Why: The `recommend_sizing` function generates at most 2 recommendations (one for CPU, one for RAM). Using `Vec::new()` meant the vector had to allocate memory dynamically when items were pushed.
📊 Impact: Eliminates up to two heap allocations per sizing recommendation run.
🔬 Measurement: Run `cargo bench -p hitz-api` (if available) or verify with `cargo test -p hitz-api --lib --no-default-features --target x86_64-unknown-linux-gnu`.
