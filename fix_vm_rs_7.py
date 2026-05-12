import re

with open("crates/hitz-vmm/src/vm.rs", "r") as f:
    code = f.read()

code = code.replace(".collect();\n\n    drop(exit_tx);", ".collect::<Result<Vec<_>, VmError>>()?;\n\n    drop(exit_tx);")

with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(code)
