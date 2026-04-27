import sys

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

# Replace the duplicated tests that were accidentally added during previous refactorings.
# It seems there are multiple copies of `test_format_error_response_json` and `test_format_error_response_plain`.
# They are scattered throughout the file.

# Instead of blindly replacing, let's just use regex to remove them, but they might be inside tests block.
import re
pattern = r'(?:\s*#\[test\]\s*fn test_format_error_response_json\(\) \{[\s\S]*?assert!\(result\.contains\("Invalid config"\)\);\s*\})'
content = re.sub(pattern, '', content)

pattern2 = r'(?:\s*#\[test\]\s*fn test_format_error_response_plain\(\) \{[\s\S]*?assert!\(result\.contains\("Not found anywhere"\)\);\s*\})'
content = re.sub(pattern2, '', content)

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(content)
