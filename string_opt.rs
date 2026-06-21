fn truncate(clean: &str, max_len: usize) -> String {
    if let Some((idx, _)) = clean.char_indices().nth(max_len) {
        format!("{}...", &clean[..idx])
    } else {
        clean.to_string()
    }
}
fn main() {
    println!("{}", truncate("Hello World", 5));
    println!("{}", truncate("Hello", 5));
    println!("{}", truncate("Hello", 10));
}
