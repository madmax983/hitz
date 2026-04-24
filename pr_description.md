Title: 🗺️ Atlas: [architectural change] Explicit Exports in hitz-api

🕸️ Tangle: The `hitz-api` crate was using wildcard exports (`pub use module::*`), which creates leaky abstractions and makes dependency tracking and API contracts ambiguous.
📐 Blueprint: Replaced all wildcard exports in `crates/hitz-api/src/lib.rs` with explicit export statements (Facade pattern), itemizing exactly which structs and functions belong to the public API contract.
🧱 Stability: Reduced coupling and improved API explicitness, which prevents accidental trait pollution and leaky implementations.
🔬 Verification: The workspace builds successfully and passes all tests without wildcard imports.
