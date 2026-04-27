import json

response_json = '{"message": "Invalid config", "code": 400}'
# wait, hitz-cli has an explicit test for `format_error_response`!
# Let's read `hitz-cli/src/main.rs` at line 2822
