import re

with open("crates/hitz-vmm/src/serial_buf.rs", "r") as f:
    code = f.read()

# Add #[allow(unexpected_cfgs)] to the top of serial_buf.rs
if "#![allow(unexpected_cfgs)]" not in code:
    code = "#![allow(unexpected_cfgs)]\n" + code

# Remove unused imports
code = code.replace("use tokio::sync::Notify;\n", "")
code = code.replace("use tokio::sync::watch;\n", "")

with open("crates/hitz-vmm/src/serial_buf.rs", "w") as f:
    f.write(code)
