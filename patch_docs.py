import re

with open('crates/hitz-api/src/lib.rs', 'r') as f:
    content = f.read()

content = content.replace("use hitz_api::simulator::{", "use hitz_api::{")

with open('crates/hitz-api/src/lib.rs', 'w') as f:
    f.write(content)

with open('crates/hitz-api/src/simulator.rs', 'r') as f:
    content = f.read()

content = content.replace("use hitz_api::simulator::{", "use hitz_api::{")

with open('crates/hitz-api/src/simulator.rs', 'w') as f:
    f.write(content)
