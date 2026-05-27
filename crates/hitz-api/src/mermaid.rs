//! Mermaid JS Topology Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid flowchart graph, allowing users to
//! visually inspect their micro-VM's resource allocations and networking topology.
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
//! let graph = config.to_mermaid("my_vm");
//! assert!(graph.contains("graph TD"));
//! assert!(graph.contains("my_vm"));
//! assert!(graph.contains("1024 MiB"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to a Mermaid JS flowchart representation.
pub trait ToMermaid {
    /// Returns the Mermaid JS graph representation as a String.
    fn to_mermaid(&self, vm_id: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, vm_id: &str) -> String {
        let mut mermaid = String::new();
        let _ = writeln!(mermaid, "graph TD");
        let _ = writeln!(mermaid, "    subgraph VM [\"Micro-VM: {vm_id}\"]");

        let _ = writeln!(mermaid, "        CPU[\"{} vCPUs\"]", self.cpus);
        let _ = writeln!(mermaid, "        RAM[\"{} MiB\"]", self.ram_mib);
        let _ = writeln!(
            mermaid,
            "        Kernel[\"{}\"]",
            self.kernel_path.display()
        );

        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(mermaid, "        Initramfs[\"{}\"]", initramfs.display());
            let _ = writeln!(mermaid, "        Kernel --> Initramfs");
        }

        if let Some(disk) = &self.disk_path {
            let _ = writeln!(mermaid, "        Disk[\"{}\"]", disk.display());
            let _ = writeln!(mermaid, "        Kernel --> Disk");
        }

        let _ = writeln!(mermaid, "    end");

        if let Some(net) = &self.net {
            let _ = writeln!(mermaid, "    subgraph Host [\"Host Network\"]");
            let _ = writeln!(
                mermaid,
                "        TAP[\"{}\"]",
                net.adapter_name.as_deref().unwrap_or("hitz-tap")
            );
            let _ = writeln!(mermaid, "        HostIP[\"{}\"]", net.host_ip);
            let _ = writeln!(mermaid, "    end");
            let _ = writeln!(mermaid, "    GuestNet[\"{}\"]", net.guest_ip);
            let _ = writeln!(mermaid, "    VM --> GuestNet");
            let _ = writeln!(mermaid, "    GuestNet --- TAP");
            let _ = writeln!(mermaid, "    TAP --- HostIP");
        }

        for port in &self.ports {
            let _ = writeln!(
                mermaid,
                "    HostPort{}[\"Host Port: {}\"] --> GuestPort{}[\"Guest Port: {}\"]",
                port.host_port, port.host_port, port.guest_port, port.guest_port
            );
            let _ = writeln!(mermaid, "    GuestPort{} --> VM", port.guest_port);
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
        assert!(m.contains("Micro-VM: test_vm"));
        assert!(m.contains("2 vCPUs"));
        assert!(m.contains("512 MiB"));
        assert!(m.contains("/boot/vmlinux"));
    }

    #[test]
    fn test_to_mermaid_with_net_and_ports() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 1,
            cmdline: None,
            net: Some(NetConfig {
                mac: None,
                host_ip: "10.0.0.1/24".to_string(),
                guest_ip: "10.0.0.2/24".to_string(),
                adapter_name: Some("hitz0".to_string()),
            }),
            ports: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
            }],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let m = config.to_mermaid("net_vm");
        assert!(m.contains("Host Network"));
        assert!(m.contains("10.0.0.1/24"));
        assert!(m.contains("10.0.0.2/24"));
        assert!(m.contains("hitz0"));
        assert!(m.contains("Host Port: 8080"));
        assert!(m.contains("Guest Port: 80"));
    }
}
