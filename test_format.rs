fn main() {
    let title = " ✗ Error (404) ";
    let details = "Not found\nSome details";
    let mut msg = format!("\n╭{}╮\n", "─".repeat(title.chars().count()));
    msg.push_str(&format!("│{title}│\n"));
    msg.push_str(&format!("├{}┤\n", "─".repeat(title.chars().count())));
    for line in details.lines() {
        msg.push_str(&format!("│ {line}\n"));
    }
    msg.push_str(&format!("╰{}╯", "─".repeat(title.chars().count())));
    println!("{}", msg);
}
