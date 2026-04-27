## 🚮 Smell
- `crates/hitz-vmm/src/vm.rs`: In `run_multi_vcpu`, there was a deeply nested `match` block around the thread result `result` that handled `Ok(Ok(ExitReason::Canceled))` as a no-op, while doing the same cleanup action for all other variants, creating a "Pyramid of Doom" that obscured intent.
- `crates/hitz-devices/src/virtio/mmio_transport.rs`: In the `write` handler, there were 6 repetitive match arms for writing the low/high values of the Queue Descriptors (`MMIO_QUEUE_DESC_LOW`, `MMIO_QUEUE_DESC_HIGH`, `MMIO_QUEUE_AVAIL_LOW`, `MMIO_QUEUE_AVAIL_HIGH`, `MMIO_QUEUE_USED_LOW`, `MMIO_QUEUE_USED_HIGH`), breaking the DRY principle.

## ✨ Solution
- **Flattened** the thread exit `match` in `run_multi_vcpu` into a single early-return-style `if !matches!(&result, Ok(Ok(ExitReason::Canceled)))` Guard Clause.
- **Extracted** the 6 `MMIO_QUEUE` address writes in `virtio/mmio_transport.rs` into a single combined match arm with a clean inner `match offset` dispatch.

## 🧼 Benefit
- Reduces cognitive load by eliminating deep nesting and boilerplate repetition.
- Enforces strict DRY and idiomatic `matches!` Rust patterns without altering any runtime behavior.

## 🛡️ Verification
- Tests passed. No logic changed.
