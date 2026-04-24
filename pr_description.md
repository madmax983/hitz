## Description
💡 What: Pre-allocated the `insights` vector with `Vec::with_capacity(2)` in `hitz-api/src/efficiency.rs` instead of using `Vec::new()`.
🎯 Why: The `calculate_efficiency` function evaluates at most two resources (CPU and Memory), resulting in a maximum of 2 insights. Pre-allocating avoids unnecessary heap reallocations as the vector grows.
📊 Impact: Eliminates dynamic heap reallocations per efficiency score calculation, which could occur frequently if called repeatedly on monitoring loops.
🔬 Measurement: Run `cargo bench` (if available) or `cargo test -p hitz-api` to ensure no logic regressions while enjoying a zero-cost abstraction.
