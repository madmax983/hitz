import os
import re

def parse_uses(filepath):
    uses = set()
    with open(filepath, 'r') as f:
        for line in f:
            if 'use crate::' in line or 'use super::' in line:
                uses.add(line.strip())
    return uses

for root, _, files in os.walk('crates'):
    for file in files:
        if file.endswith('.rs'):
            pass # We could implement a full check here, but maybe it's overkill
