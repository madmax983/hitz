//! Mermaid Diagram Generator Module.
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid flowchart (graph TD).
//! This allows users to visualize their VM topology, exposing networking,
//! ports, and resource allocation in a clear diagram.
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
//! let chart = config.to_mermaid("my_vm");
//! assert!(chart.contains("graph TD"));
//! assert!(chart.contains("my_vm [VM: my_vm]"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to a Mermaid diagram string.
pub trait ToMermaid {
    /// Returns the Mermaid flowchart representation as a String.
    fn to_mermaid(&self, vm_id: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, vm_id: &str) -> String {
        // We use String::with_capacity to avoid early reallocations
        let mut chart = String::with_capacity(512);
        let _ = writeln!(chart, "graph TD");
        let _ = writeln!(chart, "    subgraph {vm_id} [VM: {vm_id}]");
        let _ = writeln!(chart, "        {vm_id}CPU[CPUs: {}]", self.cpus);
        let _ = writeln!(chart, "        {vm_id}RAM[RAM: {} MiB]", self.ram_mib);

        if let Some(disk) = &self.disk_path {
            let _ = writeln!(chart, "        {vm_id}Disk[(Disk: {})]", disk.display());
        } else {
            let _ = writeln!(chart, "        {vm_id}Disk[(No Disk)]");
        }

        if let Some(net) = &self.net {
            let _ = writeln!(chart, "        {vm_id}Net((Net: {}))", net.guest_ip);
        }
        let _ = writeln!(chart, "    end");

        if let Some(net) = &self.net {
            let _ = writeln!(chart, "    HostNet((Host: {})) --> {vm_id}Net", net.host_ip);
        }

        for port in &self.ports {
            let _ = writeln!(
                chart,
                "    HostPort{} -->|Forwards| {vm_id}Net",
                port.host_port
            );
        }

        chart
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
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: Some(PathBuf::from("image.ext4")),
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: Some(NetConfig {
                mac: None,
                host_ip: "10.0.0.1/24".into(),
                guest_ip: "10.0.0.2/24".into(),
                adapter_name: None,
            }),
            ports: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
            }],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let chart = config.to_mermaid("test_vm");
        assert!(chart.contains("graph TD"));
        assert!(chart.contains("test_vm [VM: test_vm]"));
        assert!(chart.contains("test_vmCPU[CPUs: 4]"));
        assert!(chart.contains("test_vmRAM[RAM: 1024 MiB]"));
        assert!(chart.contains("test_vmDisk[(Disk: image.ext4)]"));
        assert!(chart.contains("test_vmNet((Net: 10.0.0.2/24))"));
        assert!(chart.contains("HostNet((Host: 10.0.0.1/24)) --> test_vmNet"));
        assert!(chart.contains("HostPort8080 -->|Forwards| test_vmNet"));
    }
}
