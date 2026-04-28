🦠 Threat: Memory safety risk in virtio-blk implementation where guest could exploit incorrect read/write descriptor permissions. Specifically, a malicious guest could provide a read-only descriptor for a read operation (where the host needs to write data) or a writable descriptor for a write operation (where the host only needs to read).

🛡️ Defense: Validated the `is_device_writable` flag on data and status descriptors during virtio-blk request processing.
1. Ensured data descriptors for `VIRTIO_BLK_T_IN` (read from disk) are device-writable.
2. Ensured data descriptors for `VIRTIO_BLK_T_OUT` (write to disk) are device-readable.
3. Ensured the status descriptor is always device-writable before attempting to write the status byte.

💥 Severity: Medium - The host might fail to complete operations silently or panic/crash if descriptor permissions are assumed instead of validated.

🧪 Verification: Added `havoc_blk_read_readonly_desc_returns_ioerr` and `havoc_blk_write_writable_desc_returns_ioerr` tests which verify that providing incorrect descriptor permissions results in a `VIRTIO_BLK_S_IOERR` instead of attempting the operation.
