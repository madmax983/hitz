//! Mermaid Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid flowchart diagram, allowing users to
//! visualize their VM configuration, including network, disk, and port forwarding.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, ToMermaid, GuestAgentMode, NetConfig};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: Some(PathBuf::from("/data/image.raw")),
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: Some(NetConfig {
//!         mac: None,
//!         host_ip: "10.0.0.1/24".to_string(),
//!         guest_ip: "10.0.0.2/24".to_string(),
//!         adapter_name: None,
//!     }),
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let diagram = config.to_mermaid("web_server");
//! assert!(diagram.contains("graph TD"));
//! assert!(diagram.contains("web_server"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid diagrams.
pub trait ToMermaid {
    /// Returns the Mermaid diagram representation as a String.
    fn to_mermaid(&self, resource_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str) -> String {
        let mut mermaid = String::new();
        let _ = writeln!(mermaid, "graph TD");

        let _ = writeln!(mermaid, "  {resource_name}[VM: {resource_name}]");

        let _ = writeln!(
            mermaid,
            "  {resource_name} --- cpus[\"CPU: {}\"]",
            self.cpus
        );
        let _ = writeln!(
            mermaid,
            "  {resource_name} --- ram[\"RAM: {} MiB\"]",
            self.ram_mib
        );

        if let Some(disk) = &self.disk_path {
            let _ = writeln!(
                mermaid,
                "  {resource_name} --- disk[\"Disk: {}\"]",
                disk.display()
            );
        }

        if let Some(net) = &self.net {
            let _ = writeln!(
                mermaid,
                "  host_net[\"Host IP: {}\"] --- {resource_name}_net[\"Guest IP: {}\"]",
                net.host_ip, net.guest_ip
            );
            let _ = writeln!(mermaid, "  {resource_name}_net --- {resource_name}");
        }

        for (i, port) in self.ports.iter().enumerate() {
            let _ = writeln!(
                mermaid,
                "  host_port_{i}[\"Host Port: {}\"] -->|Forward| guest_port_{i}[\"Guest Port: {}\"]",
                port.host_port, port.guest_port
            );
            let _ = writeln!(mermaid, "  guest_port_{i} --- {resource_name}");
        }

        mermaid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuestAgentMode, NetConfig, PortForward};
    use std::path::PathBuf;

    #[test]
    fn test_to_mermaid_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: None,
            disk_path: Some(PathBuf::from("/data/image.raw")),
            ram_mib: 512,
            cpus: 2,
            cmdline: None,
            net: Some(NetConfig {
                mac: None,
                host_ip: "10.0.0.1/24".to_string(),
                guest_ip: "10.0.0.2/24".to_string(),
                adapter_name: None,
            }),
            ports: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
            }],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let diagram = config.to_mermaid("test_vm");
        assert!(diagram.contains("graph TD"));
        assert!(diagram.contains("test_vm[VM: test_vm]"));
        assert!(diagram.contains("CPU: 2"));
        assert!(diagram.contains("RAM: 512 MiB"));
        assert!(diagram.contains("/data/image.raw"));
        assert!(diagram.contains("10.0.0.1/24"));
        assert!(diagram.contains("Host Port: 8080"));
        assert!(diagram.contains("Guest Port: 80"));
    }
}
