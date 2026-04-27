1. **Remove raw JSON parsing and formatting from error output:** I found a code block in `hitz-cli/src/main.rs` that takes a failed HTTP response, tries to parse it as `serde_json::Value`, and iterates over it, outputting raw keys and values (`k: v`) using `.fold(String::new(), ...)` separated by commas. As "Mosaic", I will replace this section with a simpler, human-readable format.
2. **Handle generic errors correctly:** If the error is a `serde_json::Value`, we'll try to extract the `message` or `error` keys to be helpful. If those don't exist, we'll avoid outputting raw JSON by providing a simple string like `✗ [Prefix] ([Status Code])`.
3. **Format PR description and submit:** Format the PR exactly as requested by Mosaic boundaries.

Complete pre commit steps to ensure proper testing, verification, review, and reflection are done.
