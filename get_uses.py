import os
import re

def parse_module(filepath, base_module):
    imports = set()
    try:
        with open(filepath, 'r') as f:
            content = f.read()
            # simple regex to find `use crate::` or `use super::` or `use hitz_`
            for match in re.findall(r'use\s+(crate::|super::|hitz_[a-z0-9_]+::)(.*?);', content):
                imports.add(match[0] + match[1])
    except Exception as e:
        pass
    return imports

for root, _, files in os.walk('crates'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            # just print files that are too long
            with open(filepath, 'r') as f:
                lines = f.readlines()
                if len(lines) > 2000:
                    print(f"Bloat: {filepath} is {len(lines)} lines long")
