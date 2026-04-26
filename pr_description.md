🦠 Threat: Unbounded memory allocation in VirtIO Block write handling could allow a malicious guest to trigger an Out-Of-Memory (OOM) panic (DoS) by sending a request with a length of `u32::MAX`. It also had a potential integer overflow in bounds checking.
🛡️ Defense: Enforced a 16MB upper bound on write requests (`len > 16_777_216`) and switched to `checked_mul` and `saturating_add` for math, ensuring the host gracefully rejects the oversized request with `VIRTIO_BLK_S_IOERR`.
💥 Severity: High - A malicious guest could crash the host process and affect the hypervisor.
🧪 Verification: Verified using the `havoc_blk_write_oom` test case which sends `u32::MAX` to the function and ensures the application gracefully rejects it.
