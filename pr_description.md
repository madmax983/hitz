🕸️ Tangle
The `WorkloadProfile` enum, which is defined in and belongs to the `simulator` module, was being incorrectly re-exported from the `classifier` module in `crates/hitz-api/src/lib.rs`. This created a leaky abstraction and an unresolved import error when building `hitz-api` with `--all-features`, as `classifier.rs` did not actually export `WorkloadProfile`.

📐 Blueprint
Removed `WorkloadProfile` from the `pub use classifier::...` export list and explicitly added it to the `pub use simulator::...` export list where it rightfully belongs.

🧱 Stability
Reduced coupling by enforcing strict domain boundaries. `WorkloadProfile` is now only exported from its source module, preventing compiler errors and maintaining a clean public API contract.

🔬 Verification
Ran `cargo check` for both Windows and Linux targets, as well as `cargo test` and `cargo clippy`. The build is successful and the leaky export is resolved.

*Note: Automated code review suggested the struct wasn't moved, but `grep` verified `WorkloadProfile` was already defined natively inside `simulator.rs` and not `classifier.rs`. No actual code movement was needed beyond fixing the leaky `pub use` statement.*
