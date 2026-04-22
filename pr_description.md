# 👺 Havoc: Prevent host OOM panic from maliciously large vsock TX descriptors

### 🧨 The Trigger
"A malicious guest with a crafted virtio-vsock driver posts descriptor lengths of `u32::MAX` to the TX queue."

### 📉 The Stack Trace
```
thread 'virtio::vsock::tests::havoc_vsock_tx_oom' panicked at crates/hitz-devices/src/virtio/vsock.rs:462:16:
attempt to add with overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
or alternatively `memory allocation of ... failed`.

### 🧪 Reproduction
Run `cargo test -p hitz-devices --lib virtio::vsock::tests::havoc_vsock_tx_oom --no-default-features --target x86_64-unknown-linux-gnu` without the fix.

### 😈 Comment
You assumed the guest would respect the maximum vsock packet sizes. You were wrong. A guest could instantly exhaust host memory or crash the VMM via overflow. Added a maximum bounds check.
