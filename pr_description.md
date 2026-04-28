🎯 Target: hitz-api sub-modules (`config.rs`, `diff.rs`, `imbalance.rs`, `efficiency.rs`, `fingerprint.rs`, `carbon.rs`, `sentinel.rs`, `classifier.rs`).
💣 Risk: Untested boundary thresholds, missing new device additions logic, untested rule logic evaluation combinations, and possible division by zero / 0-value boundaries.
🧪 Strategy: Added table-driven test cases to `sentinel.rs` for exhaustive permutations testing, and added boundary validation tests to various metric structures.
🔬 Verification: `cargo test -p hitz-api --all-features`.
Title: 🔭 Vantage: Spec for Windows-Native Guest Memory Allocator

👤 **User Story:** As a Core VMM Developer, I want a Windows-native guest memory abstraction, so that I can reliably map RAM into the microVM partition without compilation errors or POSIX compatibility issues.

✅ **Acceptance Criteria:**
- Must remove dependency on Linux-centric `vm-memory` crate.
- Must allocate memory using native Windows capabilities.
- Must map memory to the guest using native Windows Hypervisor Platform APIs.
- Must provide standard read/write accessors for guest memory.

🚫 **Out of Scope:** Memory Ballooning and NUMA awareness.

**Note:** Tested `hitz-api` individually on Linux since the workspace fails on Linux due to Windows-specific dependencies (e.g. `wintun`). This PR only introduces a documentation specification and doesn't modify application code.
💡 **The Spark:** "I noticed we collect detailed metrics and know the VM's configured size, but we don't use this combined knowledge to suggest actionable changes."
🚀 **The Feature:** "Implemented a `RightSizer` trait that analyzes `MetricsSnapshot` against `VmConfig` to emit `ResizeRecommendation` variants (scale CPU/RAM up or down)."
🔭 **The Potential:** "Could be used by the CLI to provide an `optimize` or `advise` command, or by a Kubernetes operator to autoscale micro-VMs."
⚠️ **Risk:** "Low. Isolated in `crates/hitz-api/src/rightsizer.rs` under a new `rightsizer` feature flag."
🎯 Target:
- `hitz-devices::mmio_bus`
- `hitz-devices::serial`
- `hitz-devices::virtio::block`

💣 Risk: Uncovered trait defaults, non-panicking error paths, and edge cases in block I/O (like reading out of bounds length, or missing descriptors) were lacking tests. Without these tests, changes in `VirtQueue` parsing or disk read/write bounds checks could silently regress and fail to properly protect against OS crashes or invalid guest behavior.

🧪 Strategy:
- Added a test in `mmio_bus` to verify the default implementation of `poll_rx()` returns `None`.
- Added tests in `serial` to verify `NoopTrigger` behavior and the non-panicking sink write failure handling in `pio_write()`.
- Added several new tests to `virtio::block` spanning:
  - Checking `capacity()` and `device_features()`.
  - Checking `write_config()` acts as a no-op correctly.
  - Generating descriptors with out-of-bounds length requirements, bounds check failures on `mem.write_guest()`, out-of-bounds next indexes.
  - Ensuring graceful error (`VIRTIO_BLK_S_IOERR`) is returned rather than panicked on all missing headers, failing IOs and mismatched lengths.

🔬 Verification:
`cargo test -p hitz-devices --lib --no-default-features --target x86_64-unknown-linux-gnu`
