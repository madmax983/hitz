//! Mermaid Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid.js diagram (`graph TD`), allowing users to
//! visualize their running or desired VM configurations as a topology graph.
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
//! assert!(mermaid.contains("my_vm[\"my_vm\"]"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid.js representation.
pub trait ToMermaid {
    /// Returns the Mermaid.js diagram representation as a String.
    fn to_mermaid(&self, resource_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "graph TD");
        let _ = writeln!(md, "  {resource_name}[\"{resource_name}\"]");
        let _ = writeln!(md, "  {resource_name} --> cpus[\"CPUs: {}\"]", self.cpus);
        let _ = writeln!(
            md,
            "  {resource_name} --> ram[\"RAM: {} MiB\"]",
            self.ram_mib
        );

        if let Some(disk) = &self.disk_path {
            let _ = writeln!(
                md,
                "  {resource_name} --> disk[\"Disk: {}\"]",
                disk.display()
            );
        }

        if let Some(net) = &self.net {
            let _ = writeln!(
                md,
                "  {resource_name} --> net[\"Net: {} / {}\"]",
                net.host_ip, net.guest_ip
            );
        }

        for (i, port) in self.ports.iter().enumerate() {
            let _ = writeln!(
                md,
                "  {resource_name} --> port_{i}[\"Port: {}:{}\"]",
                port.host_port, port.guest_port
            );
        }

        md
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

        let md = config.to_mermaid("test_vm");
        assert!(md.contains("graph TD"));
        assert!(md.contains("test_vm[\"test_vm\"]"));
        assert!(md.contains("CPUs: 2"));
        assert!(md.contains("RAM: 512 MiB"));
    }
}
