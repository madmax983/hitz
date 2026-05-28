fn main() {
    let mut acc = String::new();
    let k = "message";
    let s = "Invalid config";

    use std::fmt::Write;
    let _ = write!(acc, "{k}: {s}");
    println!("{}", acc);
}
