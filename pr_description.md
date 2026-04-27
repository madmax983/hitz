🎨 Mosaic: UI Polish for hitz-cli

🖌️ **Before:**
The CLI's error output function `format_error_response` would attempt to parse generic JSON errors and iterate over their key-value pairs, formatting them together. This led to outputs resembling raw JSON structures, which violates the UI design goal of clean, human-readable errors. Additionally, multiple duplicate tests for this function had been inadvertently added.

✨ **After:**
The logic has been simplified to directly extract specific `message` or `error` keys if available in the JSON response. If neither is found, it elegantly falls back to a cleaner `✗ Error Prefix (StatusCode)` message without outputting the raw JSON components to the user. The redundant duplicate tests were also cleaned up.

🖼️ **Visuals:**
Instead of `✗ Failed (400): field: invalid, error_code: 123`, the output is now cleanly simplified to display known human-readable messages or simply a concise status indication `✗ Failed (400)`.

*Note: As `hitz-cli` uses Windows-specific dependencies (e.g. `wintun`), complete local test suites cannot run on Linux. I have made the best safe assumptions and verified the logic structurally.*
