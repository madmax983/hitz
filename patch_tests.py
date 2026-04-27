import sys

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

tests = """
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
"""

content = content.replace("mod tests {", "mod tests {\n" + tests, 1)

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(content)
