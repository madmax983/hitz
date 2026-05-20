//! Mermaid JS Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Mermaid JS flowchart representation.
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
//! let md = config.to_mermaid("MyVM");
//! assert!(md.contains("graph TD"));
//! assert!(md.contains("MyVM_CPUs[\"CPUs: 4\"]"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Mermaid JS flowchart representation.
pub trait ToMermaid {
    /// Returns the Mermaid JS flowchart representation as a String.
    fn to_mermaid(&self, vm_name: &str) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, vm_name: &str) -> String {
        let mut out = String::with_capacity(1024);
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    {vm_name}[\"{vm_name}\"]");

        let _ = writeln!(
            out,
            "    {vm_name}_Kernel[\"Kernel: {}\"]",
            self.kernel_path.display()
        );
        let _ = writeln!(out, "    {vm_name} --> {vm_name}_Kernel");

        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(
                out,
                "    {vm_name}_Initramfs[\"Initramfs: {}\"]",
                initramfs.display()
            );
            let _ = writeln!(out, "    {vm_name} --> {vm_name}_Initramfs");
        }

        if let Some(disk) = &self.disk_path {
            let _ = writeln!(out, "    {vm_name}_Disk[\"Disk: {}\"]", disk.display());
            let _ = writeln!(out, "    {vm_name} --> {vm_name}_Disk");
        }

        let _ = writeln!(out, "    {vm_name}_RAM[\"RAM: {} MiB\"]", self.ram_mib);
        let _ = writeln!(out, "    {vm_name} --> {vm_name}_RAM");

        let _ = writeln!(out, "    {vm_name}_CPUs[\"CPUs: {}\"]", self.cpus);
        let _ = writeln!(out, "    {vm_name} --> {vm_name}_CPUs");

        let _ = writeln!(out, "    {vm_name}_CID[\"Guest CID: {}\"]", self.guest_cid);
        let _ = writeln!(out, "    {vm_name} --> {vm_name}_CID");

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

        let md = config.to_mermaid("TestVM");
        assert!(md.contains("graph TD"));
        assert!(md.contains("TestVM[\"TestVM\"]"));
        assert!(md.contains("TestVM_Kernel[\"Kernel: /boot/vmlinux\"]"));
        assert!(md.contains("TestVM_RAM[\"RAM: 512 MiB\"]"));
        assert!(md.contains("TestVM_CPUs[\"CPUs: 2\"]"));
    }
}
