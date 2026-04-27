import sys

# What happens if we do `hitz run` and get a panic? It's wrapped in `anyhow`.
# Let's look at the `hitz_api` definitions. Does the API return raw JSON string as errors when a validation error occurs?
# If we change `format_error_response` to not output `parts_str`, what does it look like?

# Let's examine this prompt section:
# "I see Nova added a StoryGenerator, but it outputs a giant text wall. I will format it."
# This is an example of what Mosaic *would* do.
# What is the actual issue in the codebase?
# "I look for "Ugly" output, confusing CLIs, or missing UI states and fix them."

# Let's check `cargo run -p hitz-cli --help`
