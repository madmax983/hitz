Title: ⚒️ Forge: Refactor format_error_response to flatten pyramid of doom

🚮 Smell: The `format_error_response` function in `crates/hitz-cli/src/main.rs` was a "Pyramid of Doom" with deeply nested `if/else if/else` blocks handling different parsing states (`ApiError`, generic JSON `Value`, and fallback text).
✨ Solution: Extracted the inner logic into `format_json_value_error` and `format_fallback_error` helper functions, and used Guard Clauses (early returns) to flatten the structure.
🧼 Benefit: Reduces cognitive load and improves readability by keeping functions small and focused.
🛡️ Verification: Tests passed. No logic changed.
