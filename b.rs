fn main() {
    let mut hcl = String::with_capacity(256);
    use std::fmt::Write;
    let _ = writeln!(hcl, "resource \"hitz_vm\" \"{}\" {{", "test_vm");
    println!("Initial capacity: {}", hcl.capacity());

    let _ = writeln!(hcl, "  kernel_path = \"{}\"", "/boot/vmlinux");
    println!("After 1 append capacity: {}", hcl.capacity());

    let _ = writeln!(hcl, "  ram_mib = {}", 512);
    let _ = writeln!(hcl, "  cpus = {}", 2);
    let _ = writeln!(hcl, "  guest_cid = {}", 3);
    let _ = writeln!(hcl, "}}");

    println!("Final capacity: {}", hcl.capacity());
    println!("Final len: {}", hcl.len());
}
