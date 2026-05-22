//! Mermaid.js Architecture Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid.js diagram, allowing users to
//! visually document their micro-VM architecture.
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
//! let diagram = config.to_mermaid("visual_vm");
//! assert!(diagram.contains("flowchart TD"));
//! assert!(diagram.contains("4 vCPUs"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid.js diagram representation.
pub trait ToMermaid {
    /// Returns the Mermaid.js diagram as a String.
    fn to_mermaid(&self, vm_id: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, vm_id: &str) -> String {
        let mut out = String::with_capacity(512);
        let cpus = self.cpus;
        let ram_mib = self.ram_mib;
        let kernel = self.kernel_path.display();

        let _ = writeln!(out, "flowchart TD");
        let _ = writeln!(out, "    subgraph Host [Host Machine]");
        let _ = writeln!(out, "        subgraph VM [Micro-VM: {vm_id}]");
        let _ = writeln!(out, "            CPU[\"{cpus} vCPUs\"]");
        let _ = writeln!(out, "            RAM[\"{ram_mib} MiB RAM\"]");
        let _ = writeln!(out, "            Kernel[\"{kernel}\"]");

        if let Some(disk) = &self.disk_path {
            let disk_path = disk.display();
            let _ = writeln!(out, "            Disk[\"{disk_path}\"]");
        }

        let _ = writeln!(out, "        end");

        if let Some(net) = &self.net {
            let host_ip = &net.host_ip;
            let guest_ip = &net.guest_ip;
            let _ = writeln!(
                out,
                "        HostNet[\"{host_ip}\"] <-->|vSwitch| GuestNet[\"{guest_ip}\"]"
            );
            let _ = writeln!(out, "        GuestNet --> VM");
        }

        let _ = writeln!(out, "    end");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuestAgentMode, NetConfig};
    use std::path::PathBuf;

    #[test]
    fn test_to_mermaid_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: None,
            disk_path: Some(PathBuf::from("/data/disk.img")),
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: Some(NetConfig {
                mac: None,
                host_ip: "10.0.0.1/24".to_string(),
                guest_ip: "10.0.0.2/24".to_string(),
                adapter_name: None,
            }),
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        let mermaid = config.to_mermaid("nova_vm");
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("Micro-VM: nova_vm"));
        assert!(mermaid.contains("4 vCPUs"));
        assert!(mermaid.contains("1024 MiB RAM"));
        assert!(mermaid.contains("/data/disk.img"));
    }
}
