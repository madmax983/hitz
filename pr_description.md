Title: 🗺️ Atlas: Replace glob imports with explicit facade in hitz-api

🕸️ Tangle: `hitz-api` module exports all its types via glob imports `pub use <mod>::*` which pollutes the module namespace, making dependencies hard to trace and risking name collisions.
📐 Blueprint: Replaced all `pub use <mod>::*` in `crates/hitz-api/src/lib.rs` with explicit path imports `pub use crate::<mod>::<type>` for all types defined in the crate.
🧱 Stability: Explicit imports improve compiler inference and clearly enforce module boundaries, adhering to "The Facade" architecture pattern.
🔬 Verification: Ran `cargo check`, `cargo test`, `cargo fmt`, and `cargo clippy`. No breaking changes to the API surface.
