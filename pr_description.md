🦠 Threat: Out-Of-Memory (OOM) vulnerability in Virtio Block device `handle_write`. A malicious guest could provide a descriptor length up to `u32::MAX`, causing an unbounded `vec!` allocation that crashes the VMM.
🛡️ Defense: Added a `len > 16_777_216` (16MB) bounds check at the start of `handle_write`, mirroring the defense in `handle_read`.
💥 Severity: Critical - allows an unprivileged guest to trivially DoS the host VMM.
🧪 Verification: `havoc_blk_write_oom` property test passes and safely returns an IO error without panicking.
