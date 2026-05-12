//! Build script for hitz-vmm.

#[allow(missing_docs)]
fn main() {
    println!("cargo::rustc-check-cfg=cfg(loom)");
}
