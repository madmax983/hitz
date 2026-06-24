import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

# I see what went wrong... running the replace script multiple times!
# Re-checkout from git to clear out messes.
