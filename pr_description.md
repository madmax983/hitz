🗺️ Atlas: [architectural change] explicit exports in hitz-api

🕸️ Tangle: The wildcard exports (`pub use api::*;` etc.) in `crates/hitz-api/src/lib.rs` caused trait/type pollution and made module boundaries unclear, leading to a "Shotgun" architectural smell.

📐 Blueprint: Replaced all `pub use module::*` statements with explicit explicit exports for each of the public structs, enums, constants, and traits.

🧱 Stability: Reduced coupling, cleaner namespace, and clearer module contracts. Compile times shouldn't be affected.

🔬 Verification: Builds successfully via `cargo check` and `cargo test`. All doc tests pass. Verified the file contents to ensure all explicit exports are correct.
