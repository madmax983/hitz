import sys

def process_file(filename):
    with open(filename, 'r') as f:
        lines = f.readlines()

    start_idx = -1
    end_idx = -1
    for i, line in enumerate(lines):
        if 'fn format_error_response' in line:
            start_idx = i
        if start_idx != -1 and line.startswith('}'):
            end_idx = i
            break

    if start_idx != -1 and end_idx != -1:
        print("".join(lines[start_idx:end_idx+1]))

process_file('crates/hitz-cli/src/main.rs')
