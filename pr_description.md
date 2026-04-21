Title: "🛡️ Sentry: [test coverage improvement] Hal Types and Errors"

🎯 Target: `hitz-hal::types::MmioExit`, `hitz-hal::types::IoPortExit`, and `hitz-hal::error::HalError`.
💣 Risk: Types like `MmioExit` and `IoPortExit` contain complex fields (like instruction arrays) that must format correctly for debugging exits. The `HalError` variant messages represent critical, visible system failures that must be robust to format changes. These gaps represent missed test coverage.
🧪 Strategy: Added unit tests using `#[cfg(test)] mod tests` in both files. Added explicit assertions on `format!("{:?}", exit)` for the exit types, and verified exact string output via `err.to_string()` for all variants of `HalError`.
🔭 Verification: Run `cargo test -p hitz-hal`
