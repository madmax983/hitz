🛡️ Sentry: MMIO and Serial test coverage improvement

🎯 Target: `crates/hitz-devices` (`mmio_bus.rs`, `virtio/mmio_transport.rs`, `serial.rs`)
💣 Risk: Unhandled MMIO accesses or unsupported configurations might lead to silent drops or panics if not explicitly returning deterministic values (e.g. returning 0 for unmapped registers, `None` for unmapped writes).
🧪 Strategy: Added tests for the `Default` implementations, unhandled MMIO reads/writes, unaligned configurations, and the infallibility of `NoopTrigger` inside `serial.rs`.
🔬 Verification: Run `cargo test -p hitz-devices --lib --no-default-features --target x86_64-unknown-linux-gnu`.
