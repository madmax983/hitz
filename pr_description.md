🕸️ Tangle: `hitz-api/src/lib.rs` was using wildcard exports (`pub use module::*`) extensively. This is an anti-pattern as it makes it difficult to tell where types are originating from and pollutes the public API by silently re-exporting internal details.

📐 Blueprint: Replaced all wildcard exports (`pub use api::*`, `pub use config::*`, etc.) with explicit targeted exports (`pub use api::{ActionVmRequest, ApiError...}`). Explicitly exported types using automated scripts formatting large lists over multiple lines for readability.

🧱 Stability: Strict separation enforced. No more wildcard pollution. Users and the internal crates can exactly track where `hitz_api` types originate. Build times and tooling (like `rust-analyzer`) will slightly improve.

🔭 Verification: Replaced wildcard exports using a python script, visually inspected changes, ran `cargo test -p hitz-api --lib --no-default-features --target x86_64-unknown-linux-gnu` and `cargo clippy -p hitz-api --lib --no-default-features --target x86_64-unknown-linux-gnu -- -D warnings`, verified formatting via `cargo fmt`.
