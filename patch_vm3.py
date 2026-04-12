import re

with open("crates/hitz-vmm/src/vm.rs") as f:
    content = f.read()

content = content.replace("cmdline.push('\x00');", "cmdline.push('\\0');")
content = content.replace("cmdline.push('');", "cmdline.push('\\0');")

with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(content)
