use std::sync::OnceLock;

static EXIT_COUNTER: OnceLock<u64> = OnceLock::new();

fn get_counter() -> &'static u64 {
    EXIT_COUNTER.get_or_init(|| 42)
}

fn main() {
    println!("{}", get_counter());
}
