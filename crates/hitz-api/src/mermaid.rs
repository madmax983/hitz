//! Mermaid Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid flowchart diagram, allowing users to
//! visually inspect their VM configurations.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, ToMermaid, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let flowchart = config.to_mermaid("my_vm");
//! assert!(flowchart.contains("graph TD"));
//! assert!(flowchart.contains("CPU[4 Cores]"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid diagrams.
pub trait ToMermaid {
    /// Returns the Mermaid flowchart representation as a String.
    fn to_mermaid(&self, resource_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str) -> String {
        let mut diagram = String::with_capacity(512);
        let _ = writeln!(diagram, "graph TD");
        let _ = writeln!(diagram, "  VM[{resource_name}]");
        let _ = writeln!(diagram, "  CPU[{} Cores]", self.cpus);
        let _ = writeln!(diagram, "  RAM[{} MiB]", self.ram_mib);
        let _ = writeln!(diagram, "  KERNEL[{}]", self.kernel_path.display());
        let _ = writeln!(diagram, "  VM --> CPU");
        let _ = writeln!(diagram, "  VM --> RAM");
        let _ = writeln!(diagram, "  VM --> KERNEL");
        diagram
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

        let mermaid = config.to_mermaid("test_vm");
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("VM[test_vm]"));
        assert!(mermaid.contains("CPU[2 Cores]"));
        assert!(mermaid.contains("RAM[512 MiB]"));
        assert!(mermaid.contains("KERNEL[/boot/vmlinux]"));
    }
}
