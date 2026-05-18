//! Mermaid Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid.js flowchart diagram, allowing users to
//! visualize their VM configurations.
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
//! let diagram = config.to_mermaid("my_vm");
//! assert!(diagram.contains("flowchart TD"));
//! assert!(diagram.contains("my_vm[VM: my_vm]"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid diagrams.
pub trait ToMermaid {
    /// Returns the Mermaid diagram representation as a String.
    fn to_mermaid(&self, name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, name: &str) -> String {
        let mut diagram = String::with_capacity(512);
        let _ = writeln!(diagram, "flowchart TD");
        let _ = writeln!(diagram, "  {name}[VM: {name}]");
        let _ = writeln!(diagram, "  cpus[CPUs: {}]", self.cpus);
        let _ = writeln!(diagram, "  ram[RAM: {} MiB]", self.ram_mib);
        let _ = writeln!(diagram, "  {name} --> cpus");
        let _ = writeln!(diagram, "  {name} --> ram");
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

        let diagram = config.to_mermaid("test_vm");
        assert!(diagram.contains("flowchart TD"));
        assert!(diagram.contains("test_vm[VM: test_vm]"));
        assert!(diagram.contains("cpus[CPUs: 2]"));
        assert!(diagram.contains("ram[RAM: 512 MiB]"));
    }
}
