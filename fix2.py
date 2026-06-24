import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

content = content.replace("color_for_pct(proc.cpu_pct)", "color_for_pct(proc.cpu_pct.into())")
content = content.replace("make_bar(snap.cpu.total_pct, 15)", "make_bar(snap.cpu.total_pct.into(), 15)")
content = content.replace("color_for_pct(snap.cpu.total_pct)", "color_for_pct(snap.cpu.total_pct.into())")

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(content)
