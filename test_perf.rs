fn main() {
    let mut s1 = format!("resource \"hitz_vm\" \"test\" {{\n");
    s1.push_str("  kernel_path = \"test\"\n");
    s1.push_str("}\n");

    let mut s2 = String::with_capacity(128);
    s2.push_str("resource \"hitz_vm\" \"");
    s2.push_str("test");
    s2.push_str("\" {\n");
    s2.push_str("  kernel_path = \"test\"\n");
    s2.push_str("}\n");
}
