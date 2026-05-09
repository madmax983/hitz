💡 What: Optimized `format_error_response` in `hitz-cli/src/main.rs`. Replaced `clean.chars().take(max_len).collect()` with `clean.char_indices().nth(max_len)` to avoid a `String` heap allocation on error message truncation.
🎯 Why: Creating a completely new `String` via `collect()` just to add an ellipsis is an unnecessary allocation, especially since `clean` is already in memory. By finding the byte boundary and slicing it, we get the exact same result with zero allocation.
📊 Impact: Eliminates an `O(N)` character count loop and an `O(N)` heap allocation for `String` collection every time a long error message is displayed.
🔬 Measurement: Run `cargo check -p hitz-cli --bin hitz --target x86_64-pc-windows-msvc`.
