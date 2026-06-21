fn truncate1(clean: &str, max_len: usize) -> String {
    if clean.chars().count() > max_len {
        let truncated: String = clean.chars().take(max_len).collect();
        format!("{}...", truncated)
    } else {
        clean.to_string()
    }
}
fn truncate2(clean: &str, max_len: usize) -> String {
    if let Some((idx, _)) = clean.char_indices().nth(max_len) {
        format!("{}...", &clean[..idx])
    } else {
        clean.to_string()
    }
}
fn main() {
    let s = "Hello World! This is a longer string that will be truncated.";
    println!("1: {}", truncate1(s, 20));
    println!("2: {}", truncate2(s, 20));
}
