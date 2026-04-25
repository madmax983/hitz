import re
import sys

with open("crates/hitz-cli/src/main.rs", "r") as f:
    content = f.read()

search = """                    .header(
                        Row::new(["Device", "Read KB", "Write KB"]).style(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                                .add_modifier(Modifier::UNDERLINED),
                        ),
                    )"""

if search in content:
    print("Found updated format already in file.")
