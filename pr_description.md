Title: ⚡ Bolt: Removed intermediate `.collect::<Vec<_>>()` and `format!` heap allocations

💡 What: Replaced intermediate `Vec` allocations and multiple string `format!` heap allocations with a `.fold(String::new(), ...)` pattern and `std::fmt::Write` in `hitz-cli/src/main.rs`.
🎯 Why: Constructing intermediate `Vec` collections and using string `format!` in loops creates unnecessary heap allocations and pressure on the allocator, especially when the goal is just to produce a single formatted `String` for terminal output.
📊 Impact: Eliminates multiple dynamic heap allocations (intermediate `Vec`s and intermediate `String`s from `format!`) when parsing API error responses and formatting health status transition outputs in the CLI.
🔬 Measurement: Run `cargo check` and review the code in `hitz-cli/src/main.rs`. Run `cargo test -p hitz-cli --lib --no-default-features --target x86_64-unknown-linux-gnu` (or similar depending on platform) to verify behavior is identical.
