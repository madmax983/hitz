import sys

file_path = "crates/hitz-devices/src/virtio/queue.rs"
with open(file_path, "r") as f:
    content = f.read()

# I embedded the impl inside `impl DescriptorChain` which is a syntax error.
# Let's restore the file first
