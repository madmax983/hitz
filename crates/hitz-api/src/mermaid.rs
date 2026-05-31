//! Mermaid.js Architecture Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid.js graph definition, allowing users to
//! visually map their VM topologies and resource boundaries.
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
//! let mermaid = config.to_mermaid("my_vm");
//! assert!(mermaid.contains("graph TD"));
//! assert!(mermaid.contains("my_vm"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid.js diagram representation.
pub trait ToMermaid {
    /// Returns the Mermaid.js graph representation as a String.
    fn to_mermaid(&self, resource_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str) -> String {
        let mut out = "graph TD\n".to_string();
        let _ = writeln!(out, "  subgraph {resource_name}");

        // Compute resources
        let _ = writeln!(out, "    Compute[vCPUs: {}]:::resource", self.cpus);
        let _ = writeln!(out, "    Memory[RAM: {} MiB]:::resource", self.ram_mib);

        // Storage
        if self.disk_path.is_some() {
            let _ = writeln!(out, "    Disk[Disk Configured]:::storage");
        }

        // Boot
        let _ = writeln!(
            out,
            "    Kernel[Kernel: {}]:::boot",
            self.kernel_path.display()
        );
        if self.initramfs_path.is_some() {
            let _ = writeln!(out, "    Initramfs[Initrd Configured]:::boot");
        }

        // Network
        if self.net.is_some() {
            let _ = writeln!(out, "    Net[Network Configured]:::network");
        }

        // Ports
        if !self.ports.is_empty() {
            let _ = writeln!(
                out,
                "    Ports[Forwarded Ports: {}]:::network",
                self.ports.len()
            );
        }

        let _ = writeln!(out, "    VSock[VSock CID: {}]:::network", self.guest_cid);

        let _ = writeln!(out, "  end");
        let _ = writeln!(
            out,
            "  classDef resource fill:#f9f,stroke:#333,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "  classDef storage fill:#bbf,stroke:#333,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "  classDef boot fill:#bfb,stroke:#333,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "  classDef network fill:#fbb,stroke:#333,stroke-width:2px;"
        );
        out
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

        let m = config.to_mermaid("test_vm");
        assert!(m.contains("graph TD"));
        assert!(m.contains("subgraph test_vm"));
        assert!(m.contains("Compute[vCPUs: 2]"));
        assert!(m.contains("Memory[RAM: 512 MiB]"));
    }
}
