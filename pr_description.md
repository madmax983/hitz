🎯 Target: hitz-api sub-modules (`config.rs`, `diff.rs`, `imbalance.rs`, `efficiency.rs`, `fingerprint.rs`, `carbon.rs`, `sentinel.rs`, `classifier.rs`).
💣 Risk: Untested boundary thresholds, missing new device additions logic, untested rule logic evaluation combinations, and possible division by zero / 0-value boundaries.
🧪 Strategy: Added table-driven test cases to `sentinel.rs` for exhaustive permutations testing, and added boundary validation tests to various metric structures.
🔬 Verification: `cargo test -p hitz-api --all-features`.
