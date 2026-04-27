import sys

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

# Let's locate format_error_response
start = content.find('fn format_error_response(status: hyper::StatusCode, resp: &str, error_prefix: &str) -> String {')
if start == -1:
    print("Cannot find function")
    sys.exit(1)

# we need to replace until `format!("\r\x1b[2K{}", msg)\n}`
end_str = '    format!("\\r\\x1b[2K{}", msg)\n}'
end = content.find(end_str, start)
if end == -1:
    print("Cannot find end")
    sys.exit(1)
end += len(end_str)

new_func = """fn format_error_response(status: hyper::StatusCode, resp: &str, error_prefix: &str) -> String {
    let msg = if let Ok(err) = serde_json::from_str::<hitz_api::ApiError>(resp) {
        format!("✗ {error_prefix}: {}", err.message)
    } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(resp) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            format!("✗ {error_prefix}: {}", msg)
        } else if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            format!("✗ {error_prefix}: {}", err)
        } else {
            format!("✗ {error_prefix} ({status})")
        }
    } else {
        let clean = resp.trim();
        if clean.is_empty() {
            format!("✗ {error_prefix} ({status})")
        } else {
            let max_len = 200;
            let display_text = if clean.chars().count() > max_len {
                let truncated: String = clean.chars().take(max_len).collect();
                format!("{}...", truncated)
            } else {
                clean.to_string()
            };
            format!("✗ {error_prefix} ({status}): {display_text}")
        }
    };
    format!("\\r\\x1b[2K{}", msg)
}"""

content = content[:start] + new_func + content[end:]

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(content)
print("patched")
