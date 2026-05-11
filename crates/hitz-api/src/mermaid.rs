//! Mermaid Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid flowchart diagram, allowing users to
//! visualize their micro-VM configurations.
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
//! assert!(diagram.contains("graph TD"));
//! assert!(diagram.contains("my_vm"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid flowchart representation.
pub trait ToMermaid {
    /// Returns the Mermaid flowchart representation as a String.
    fn to_mermaid(&self, resource_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str) -> String {
        let mut mermaid = String::new();
        let _ = writeln!(mermaid, "graph TD");
        let _ = writeln!(mermaid, "    VM[{resource_name} (VM)]");
        let _ = writeln!(mermaid, "    Kernel[\"{}\"]", self.kernel_path.display());
        let _ = writeln!(mermaid, "    VM --> Kernel");

        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(mermaid, "    Initramfs[\"{}\"]", initramfs.display());
            let _ = writeln!(mermaid, "    VM --> Initramfs");
        }
        if let Some(disk) = &self.disk_path {
            let _ = writeln!(mermaid, "    Disk[\"{}\"]", disk.display());
            let _ = writeln!(mermaid, "    VM --> Disk");
        }

        let _ = writeln!(
            mermaid,
            "    Resources[{} vCPUs, {} MiB RAM]",
            self.cpus, self.ram_mib
        );
        let _ = writeln!(mermaid, "    VM --> Resources");

        if let Some(_net) = &self.net {
            let _ = writeln!(mermaid, "    Network[Configured]");
            let _ = writeln!(mermaid, "    VM --> Network");
        }

        mermaid
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
        assert!(diagram.contains("graph TD"));
        assert!(diagram.contains("test_vm"));
        assert!(diagram.contains("VM --> Kernel"));
        assert!(diagram.contains("512 MiB RAM"));
        assert!(diagram.contains("2 vCPUs"));
    }
}
