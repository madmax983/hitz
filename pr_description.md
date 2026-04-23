# 🛡️ Sentry: [test coverage improvement]

## 🎯 Target
`crates/hitz-vmm/src/run_loop.rs` specifically handling of `VcpuExit::Mmio` read and writes in `handle_mmio_read` and `handle_mmio_write`.

## 💣 Risk
When handling `VcpuExit::Mmio` exits for 64-bit platforms, an attacker or faulty guest could issue a write instruction where the decoded size could theoretically exceed the expected maximum of 8 bytes on x86_64, or the VM execution state may be manipulated into incorrectly presenting an `MmioExit` instruction size of over 8 bytes.

The current run loop implementation blindly assumes the decoded instruction size is <= 8. It initializes a buffer `let mut data = [0u8; 8];`, and subsequently calls `data[..size].copy_from_slice(...)` or passes `&mut data[..size]` into `devs.mmio_bus.read(...)`.
If the length of `data` (8 bytes) is exceeded by `size`, the slice accesses will panic, crashing the `hitz` host daemon.

## 🧪 Strategy
Validating the slice index bounds checks via `let size = usize::from(decoded.size); if size > 8 { return Err(HalError::InvalidMmioSize(size)); }` immediately prior to slice operations. By surfacing an explicit `InvalidMmioSize` error code (which was added to `HalError`), we ensure the hypervisor elegantly shuts down or isolates the error instead of hard-panicking the host.

## 🔬 Verification
Run `cargo test -p hitz-vmm --lib --no-default-features array_bounds_in_run_loop` or view tests covering this explicit size check.
