# 🔒 Warden: [security fix]

## 🦠 Threat
Integer overflow in virtio block handle_write offset calculation. A user could send a massive `sector` combined with a large `len` to overflow `byte_offset.saturating_add(data_len)`, bypassing bounds checks and allowing arbitrary writes/reads across the disk or crashing the service via out-of-bounds `Seek`.

## 🛡️ Defense
Implemented bounds checks using `capacity_bytes` computed from the disk's capacity and capped max requests to `16MB` to prevent out of memory panics from `vec![0u8; len as usize]`. Updated `handle_write` to match `handle_read`'s secure math calculations using `checked_mul` and `saturating_add`.

## 💥 Severity
Critical - could panic the server (OOM) or allow arbitrary disk manipulation and potential data loss.

## 🧪 Verification
Added verification logic to `havoc_blk_read_oom` and `havoc_blk_write_oom` that tests `u32::MAX` length requests. Tests verify that an `VIRTIO_BLK_S_IOERR` is cleanly returned without crashing the process.
