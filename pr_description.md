Title: 🛡️ Sentry: [test coverage improvement]

🎯 Target: `virtio::mmio_transport` module in `hitz-devices`
💣 Risk: Missing test coverage for edge cases like out-of-bounds writes to MMIO registers, unhandled read/write handling, and proper state transition logic.
🧪 Strategy: Added 13 new unit tests to cover missing read_reg/write_reg cases, paging logic, queue bounds checks, and unaligned writes/reads.
🔬 Verification: `cargo test -p hitz-devices --lib --no-default-features --target x86_64-unknown-linux-gnu`
