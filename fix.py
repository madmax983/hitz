import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

# Fix the duplicate tests and syntax errors
content = content.replace("            return result.map_or_else(", "            result.map_or_else(")

content = re.sub(r'#[cfg\(test\)]\s*mod tests \{.*?test_parse_port_forward.*?\}', '', content, flags=re.DOTALL)
content = re.sub(r'fn test_format_error_response_json\(\) \{.*?(?=\})\}', '', content, flags=re.DOTALL)
content = re.sub(r'fn test_format_error_response_plain\(\) \{.*?(?=\})\}', '', content, flags=re.DOTALL)

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(content)
