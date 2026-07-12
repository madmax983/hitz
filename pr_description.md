🎨 **Before:** CLI error outputs used raw debug formatting `{e:#}`, which dumped unstructured multi-line strings or OS panics like "TCP Reset" directly to the console, confusing users. Additionally, there were multiple duplicated test blocks `test_format_error_response_json` and `test_format_error_response_plain` scattered throughout the codebase (likely artifacts from past copy-paste operations).

✨ **After:** Errors are now cleanly formatted with `format_anyhow_error(&e)` which iterates through the `anyhow` error chain and neatly nests subsequent causes using an elegant `↳` prefix, giving the user full debug context without the messy debug string format. The incorrectly duplicated test blocks were cleanly removed, while retaining the correct ones inside the proper `mod tests { ... }` block to preserve coverage.

🖼️ **Visuals:** Errors now look like this:
```
✗ Error: Failed to start hitz daemon
  ↳ address already in use
  ↳ OS error 98
```
(Instead of a raw dump)
