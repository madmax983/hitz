fn f1(s: &str) -> char {
    s.chars().next().unwrap_or('?')
}
fn f2(s: &str) -> char {
    s.as_bytes().first().map(|&b| b as char).unwrap_or('?')
}
fn main() {
    println!("{}", f1("R"));
    println!("{}", f2("R"));
}
