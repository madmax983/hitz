#![allow(clippy::module_name_repetitions)]

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to a Mermaid flowchart.
pub trait ToMermaid {
    /// Returns the Mermaid flowchart representation as a String.
    fn to_mermaid(&self) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self) -> String {
        let mut chart = String::new();
        let _ = writeln!(chart, "flowchart TD");
        let _ = writeln!(chart, "    VM[Virtual Machine]");
        let _ = writeln!(chart, "    Kernel[Kernel: {}]", self.kernel_path.display());
        let _ = writeln!(chart, "    VM --> Kernel");
        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(chart, "    Initramfs[Initramfs: {}]", initramfs.display());
            let _ = writeln!(chart, "    VM --> Initramfs");
        }
        let _ = writeln!(chart, "    RAM[RAM: {} MiB]", self.ram_mib);
        let _ = writeln!(chart, "    VM --> RAM");
        let _ = writeln!(chart, "    CPUs[CPUs: {}]", self.cpus);
        let _ = writeln!(chart, "    VM --> CPUs");
        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_to_mermaid_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 512,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        let diagram = config.to_mermaid();
        assert!(diagram.contains("flowchart TD"));
        assert!(diagram.contains("Kernel: /boot/vmlinux"));
        assert!(diagram.contains("RAM: 512 MiB"));
        assert!(diagram.contains("CPUs: 2"));
    }
}
