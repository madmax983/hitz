trait ToTerraform {
    fn to_terraform(&self, resource_name: &str) -> String;
}

struct VmConfig;

impl ToTerraform for VmConfig {
    /// ⚡ Bolt Optimization: Uses `String::with_capacity()` instead of `format!()`
    /// to initialize the HCL buffer, avoiding multiple heap reallocations when
    /// incrementally building the output string via `writeln!`.
    fn to_terraform(&self, resource_name: &str) -> String {
        let mut hcl = String::with_capacity(256);
        use std::fmt::Write;
        let _ = writeln!(hcl, "resource \"hitz_vm\" \"{}\" {{", resource_name);
        hcl
    }
}

fn main() {}
