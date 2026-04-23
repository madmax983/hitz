# 🔒 Warden: Security Fixes for Virtio Devices (OOM & Integer Overflows)

## 🦠 Threat
The `hitz-devices` crate processes untrusted inputs directly from the guest VM via virtio descriptors. In `virtio/net.rs` and `virtio/block.rs`, the host explicitly trusted the guest-provided `desc.len` when resizing internal buffers or allocating vectors (`vec![0u8; len as usize]`). A malicious guest providing a massive `desc.len` (e.g., `u32::MAX`) could easily exhaust host memory, causing an Out-Of-Memory (OOM) panic and a Denial-of-Service (DoS) condition on the entire VMM.

Additionally, in `virtio/block.rs`, an integer overflow existed where `sector * SECTOR_SIZE` was evaluated *before* validating that `sector` was within the disk capacity. A massive `sector` value provided by the guest could cause a host panic when compiled with overflow checks enabled.

## 🛡️ Defense
- **Contain (virtio-net):** Implemented a safety cap on `desc.len` accumulation in `virtio/net.rs`. Host buffers will now gracefully `break` and stop consuming descriptors if the accumulated frame size exceeds a realistic maximum (65536 bytes).
- **Sanitize (virtio-block):** Enforced a hard 16MB maximum on incoming read/write request lengths in `virtio/block.rs`.
- **Sanitize (virtio-block):** Moved the calculation of `byte_offset` *after* validating that `sector < self.capacity`.
- **Fortify (virtio-block):** Explicitly implemented `saturating_add` and `checked_mul` bounds checking to ensure offset computations cannot integer overflow, falling back to safe limits that will trigger standard bounds errors.

## 💥 Severity
Critical - An untrusted guest could intentionally trigger an OOM panic or an integer overflow panic on the host, crashing the VMM and potentially affecting neighboring partitions.

## 🧪 Verification
- `cargo audit` passed (no new dependency vulnerabilities).
- Implemented test cases (`havoc_net_tx_oom`, `havoc_blk_read_oom`, and `havoc_blk_write_oom`) that explicitly send massively large bounds. The application now bounds the sizes correctly and returns `VIRTIO_BLK_S_IOERR` or truncates gracefully without panicking.
