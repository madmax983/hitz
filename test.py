import sys
import json

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    lines = f.readlines()

for i, line in enumerate(lines):
    if 'format_error_response' in line:
        print(f"Line {i+1}: {line.strip()}")
