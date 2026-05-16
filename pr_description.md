💡 What: Replaced `String::new()` with `String::with_capacity(64)` and `String::with_capacity(128)` in `crates/hitz-cli/src/main.rs` where strings are built incrementally using `.fold()` and the `write!` macro.
🎯 Why: Using `String::new()` when incrementally building a string via the `write!` macro causes an immediate heap reallocation upon the first subsequent append.
📊 Impact: Eliminates multiple heap reallocations per formatted string on error responses and health transition rendering in the CLI, reducing memory overhead and latency.
🔬 Measurement: Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` to verify behavior remains correct and idiomatic.
