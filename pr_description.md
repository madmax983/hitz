🗺️ Atlas: [architectural change]

* 🕸️ Tangle: The `hitz-api` crate uses `pub use module::*;` extensively, which causes trait pollution and leaks private implementation details.
* 📐 Blueprint: Replaced all wildcard exports in `crates/hitz-api/src/lib.rs` with explicit, enumerated exports.
* 🧱 Stability: Enforces clean, strict public API boundaries, avoiding accidental public exposure.
* 🔬 Verification: Built with `cargo test -p hitz-api --lib --no-default-features --target x86_64-unknown-linux-gnu` and `cargo check`.
