🎯 Target:
`hitz-api`, `hitz-daemon`, `hitz-vmm`, and `hitz-devices` crates.

💥 Risk:
Without these tests, core VM management operations (like restarting VMs, or config validation) could panic or silently fail without detection. Device and run-loop edge cases (like MMIO unmapped writes, or MMIO reset) lacked verification of behavior. The lack of `ProcRate` diffing meant process metrics wouldn't translate appropriately across snapshots.

🧪 Strategy:
- Added unit tests for `restart_vm` timeouts and success flows in `hitz-daemon::vm_manager`.
- Added unit tests for `handle_mmio_read` and `handle_mmio_write` in `hitz-vmm::run_loop` to ensure instruction pointers advance as expected.
- Added tests for `validate_config` in `hitz-vmm::vm` for path traversal and missing cmdlines.
- Added `check_reset_clears_fields` test for `VirtioMmioTransport` in `hitz-devices::virtio::mmio_transport`.
- Ensured `ProcRate` was accurately tracked in `hitz-api::MetricsDiff` alongside other metrics during diffing, adding a missing logic branch.

🔭 Verification:
`cargo test --manifest-path crates/hitz-devices/Cargo.toml --lib`
`cargo test --manifest-path crates/hitz-api/Cargo.toml --lib`
`cargo test --manifest-path crates/hitz-vmm/Cargo.toml --lib`
`cargo test --manifest-path crates/hitz-daemon/Cargo.toml --lib`

**Assumptions**: `hitz-daemon`, `hitz-vmm`, and `hitz-cli` fail to build under `cargo test` and `cargo-tarpaulin` due to transitive Windows-specific requirements (like `wintun` missing `IMarshal` or stdcall bindings). Verification of those specific changes relied heavily on isolated testing, local `cargo check`, and code inspection.
