🦠 Threat: Memory corruption vulnerability in virtio-blk where the device did not validate the write flags of data descriptors, allowing the guest to spoof descriptor writability.
🛡️ Defense: Explicitly validating `is_device_writable` on data descriptors based on request type (IN requires writable, OUT requires read-only).
💥 Severity: Critical - could lead to arbitrary guest memory corruption or DoS.
🧪 Verification: Added fuzzing test cases `havoc_blk_read_readonly_desc` and `havoc_blk_write_writable_desc`.
