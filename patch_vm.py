import re
with open('crates/hitz-vmm/src/vm.rs', 'r') as f:
    content = f.read()

content = content.replace('let msg = payload.downcast_ref::<&str>().map_or_else(', 'let msg = (*payload).downcast_ref::<&\'static str>().copied().map_or_else(')

with open('crates/hitz-vmm/src/vm.rs', 'w') as f:
    f.write(content)
