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
