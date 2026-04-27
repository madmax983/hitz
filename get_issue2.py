import sys

# The error output currently looks like:
# format!("✗ {error_prefix} ({status}): {parts_str}")
# Where parts_str is just `key: value, key2: value2`. This is very close to raw JSON.
# Wait, let's look at `parts_str`:
# obj.iter().fold(String::new(), |mut acc, (k, val)| { ... })
# This outputs exactly `key: value, key2: value2`. This is not very "human readable".
# Or maybe the problem is that it falls back to raw string output:
# let display_text = if clean.chars().count() > max_len {
#    let truncated: String = clean.chars().take(max_len).collect();
#    format!("{}...", truncated)
# } else {
#    clean.to_string()
# };
# This prints `✗ Error (500): {"raw":"json"}` directly to the user if it's not valid JSON, or if it doesn't parse as a JSON object!

# But wait, earlier, in the memory or prompts, we are told:
# "Never output raw JSON to the user console unless requested."
# "Errors: Wrap raw errors in human-readable messages (e.g., "Connection Failed" instead of "TCP Reset")."

# Let's read `hitz-cli/src/main.rs` again at `run_vm`
# Command::Run(args) => run_vm(args).unwrap_or_else(|e| {
#    eprintln!("\r\x1b[2K{}", format!("✗ Error: {e:#}").red().bold());
