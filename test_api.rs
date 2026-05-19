use hitz_api::VmAction;

fn main() {
    let a = VmAction::Start;
    println!("{}", a.gerund());
    println!("{}", a.past_tense());
}
