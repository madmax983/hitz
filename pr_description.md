🚮 Smell: The `WorkloadProfile` enum was exported from the `classifier` module in `crates/hitz-api/src/lib.rs`, but it is actually defined in the `simulator` module, causing a broken build (`unresolved import classifier::WorkloadProfile`).

✨ Solution: Updated the exports in `crates/hitz-api/src/lib.rs`. Moved `WorkloadProfile` out of the `classifier` block and added it to the `simulator` block.

🧼 Benefit: Fixes the broken build and ensures types are exported from their correct origin module, improving code organization and readability.

🛡️ Verification: Tests passed. No logic changed. Build is clean without warnings.
