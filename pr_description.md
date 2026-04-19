Title: 🎨 Mosaic: UI Polish for hitz-cli error responses

🖌️ **Before:**
The CLI's error formatter (`format_error_response`) and error fallbacks in JSON parsing dumped raw JSON API responses to the user console. A failure to parse the `/vms` list would print `✗ Failed to parse VMs list: {"error": ...}` resulting in a giant, ugly text wall. Unrecognized JSON payloads generated ugly logs instead of looking like a dashboard.

✨ **After:**
The CLI now intercepts raw JSON errors, automatically extracts top-level fields (like `error` or arbitrary keys), and formats them into a clean, human-readable list (e.g. `key: value`). Giant HTML error dumps from gateways are safely truncated.

🖼️ **Visuals:**
JSON dumps are replaced with structured properties. `print_error_response` is now strictly used across parsing boundaries to prevent unformatted panics.
