# ⚒️ Forge: CLI Refactor - Display trait implementations and match duplication removal

## 🚮 Smell
The `crates/hitz-cli/src/main.rs` file contained several instances of duplicated match statements for enumerations such as `VmState`, `HealthStatus`, and `GuestAgentMode`. Also, there were multiple `#[cfg(test)] mod tests { ... }` blocks containing exactly the same tests over and over at the end of the file.

## ✨ Solution
- Implemented the `std::fmt::Display` trait for `VmState` and `GuestAgentMode` in the `hitz-api` crate, moving string conversion out of the CLI's presentation logic.
- Implemented `std::fmt::Display` for `HealthStatus` in the `hitz-api` crate to avoid duplicating code.
- Created small, focused UI helper functions (`state_to_cell`, `state_to_color`, `health_to_cell`, `warning_level_to_cell`) in `main.rs` to abstract away the logic of turning these states into formatted UI components (`comfy_table::Cell` and `ratatui::style::Color`).
- Removed the multiple redundant occurrences of `mod tests` blocks and duplicates of `test_format_error_response_json` and `test_format_error_response_plain`.

## 🧼 Benefit
Reduces cognitive load by replacing boilerplate code with clean function calls or `.to_string()` invocations. Keeps formatting logic encapsulated. Cleans up the test module structure ensuring no duplicate functions are running.

## 🛡️ Verification
Ran `cargo clippy --workspace --all-targets --all-features --exclude hitz-whp --exclude hitz-cli --exclude hitz-daemon --exclude hitz-net --exclude hitz-devices --exclude hitz-vmm --exclude hitz-guest-agent -- -D warnings` and `cargo test --workspace --exclude hitz-whp --exclude hitz-cli --exclude hitz-daemon --exclude hitz-net --exclude hitz-devices --exclude hitz-vmm --exclude hitz-guest-agent`. Tests passed. No logic changed.
