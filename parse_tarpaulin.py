import json

with open("tarpaulin-report.json", "r") as f:
    data = json.load(f)

for file in data.get('files', []):
    path_parts = file.get('path', [])
    path = '/'.join(path_parts)

    if 'hitz-api' not in path:
        continue

    uncovered_lines = []
    covered_lines = []
    for trace in file.get('traces', []):
        if trace.get('stats', {}).get('Line', 0) == 0:
            uncovered_lines.append(str(trace.get('line')))
        else:
            covered_lines.append(str(trace.get('line')))

    print(f"{path}: covered: {len(covered_lines)}, uncovered: {len(uncovered_lines)}")
    if uncovered_lines:
        print(f"  uncovered lines: {', '.join(uncovered_lines)}")
