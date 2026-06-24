import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

# I am undoing the mess from `replace_draw_vm_top_ui.py` which was clearly bad. I'll just rewrite it via regex cleanly.
