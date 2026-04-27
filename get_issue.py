import sys

# The prompt asks Mosaic to fix the UI output.
# "I look for "Ugly" output, confusing CLIs, or missing UI states and fix them."
# "Never output raw JSON to the user console unless requested."

# Right now `format_error_response` takes JSON and creates an ugly comma-separated list of keys and values if it is not an ApiError and does not have message or error keys.
# Actually, wait, `parts_str` prints keys and values but misses visual hierarchy.
# Or maybe the entire `run` command output?
# Let's see what happens if I parse `ApiError`... Oh, actually I shouldn't just "fix the JSON formatting" maybe?

# What if I change `format_error_response` to just output:
# `✗ Failed (400): {"field": "invalid"}` -> Wait, it currently outputs `✗ Failed (400): field: invalid`
# Is that raw JSON?

# Wait, `cargo run -p hitz-cli --example` might not exist, but let's check `cargo run -p hitz-cli -- vm analyze nonexistent`.
