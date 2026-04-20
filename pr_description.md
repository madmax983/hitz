Title: ⚒️ Forge: format_error_response
Description:
- 🚮 Smell: Deeply nested if-else match statements for JSON parsing in format_error_response
- ✨ Solution: Extracted the logic for parsing and mapping to a new helper function extract_json_error, applying early returns and chaining in a clear manner.
- 🧹 Benefit: Code is flattened, distinct paths and cases are straightforward and explicit.
- 🛡️ Verification: The changes have been validated by formatting the code and ensuring that no changes strictly modified functionality (all paths remained similar while separating the steps of formatting vs string extracting).
