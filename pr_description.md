🎯 Target: `hitz-api`, `hitz-vmm`, `hitz-devices`, `hitz-daemon`
💥 Risk: Usage of `.unwrap()` on dynamic data could cause panics. Replaced with `.expect()` containing a custom panic message to track the source of the crash. Fixed clippy errors and made `simulator` module public. Fixed `classifier` export. Addressed tests and production code.
🧪 Strategy: Tracked down missing coverage and unsafe unwraps in the entire workspace. Modified both production and test files.
🔬 Verification: Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test --all-features`. Note that `hitz-vmm`, `hitz-devices`, and `hitz-daemon` fail to compile on Linux due to the `wintun` dependency.
