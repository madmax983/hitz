//! Terraform HCL Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Terraform HCL resource block, allowing users to
//! export their running or desired VM configurations into Infrastructure-as-Code.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, ToTerraform, GuestAgentMode};
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
//! let hcl = config.to_terraform("my_vm");
//! assert!(hcl.contains("resource \"hitz_vm\" \"my_vm\""));
//! assert!(hcl.contains("cpus = 4"));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Terraform HCL representation.
pub trait ToTerraform {
    /// Generates the raw Terraform HCL configuration string for the resource.
    ///
    /// # Abstract
    /// Transforms the internal configuration into a deployable Infrastructure-as-Code
    /// block, using the provided `resource_name` as the Terraform identifier.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{VmConfig, ToTerraform, GuestAgentMode};
    /// use std::path::PathBuf;
    ///
    /// let config = VmConfig {
    ///     kernel_path: PathBuf::from("vmlinux"),
    ///     initramfs_path: None,
    ///     disk_path: None,
    ///     ram_mib: 512,
    ///     cpus: 1,
    ///     cmdline: None,
    ///     net: None,
    ///     ports: vec![],
    ///     guest_cid: 3,
    ///     guest_agent: GuestAgentMode::Auto,
    /// };
    ///
    /// let hcl = config.to_terraform("my_resource");
    /// assert!(hcl.starts_with("resource \"hitz_vm\" \"my_resource\" {\n"));
    /// ```
    fn to_terraform(&self, resource_name: &str) -> String;
}

impl ToTerraform for VmConfig {
    fn to_terraform(&self, resource_name: &str) -> String {
        let mut hcl = format!("resource \"hitz_vm\" \"{resource_name}\" {{\n");
        let _ = writeln!(hcl, "  kernel_path = \"{}\"", self.kernel_path.display());

        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(hcl, "  initramfs_path = \"{}\"", initramfs.display());
        }
        if let Some(disk) = &self.disk_path {
            let _ = writeln!(hcl, "  disk_path = \"{}\"", disk.display());
        }
        let _ = writeln!(hcl, "  ram_mib = {}", self.ram_mib);
        let _ = writeln!(hcl, "  cpus = {}", self.cpus);

        if let Some(cmdline) = &self.cmdline {
            let _ = writeln!(hcl, "  cmdline = \"{cmdline}\"");
        }

        let _ = writeln!(hcl, "  guest_cid = {}", self.guest_cid);
        let _ = writeln!(hcl, "}}");

        hcl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_to_terraform_basic() {
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

        let hcl = config.to_terraform("test_vm");
        assert!(hcl.contains("resource \"hitz_vm\" \"test_vm\" {"));
        assert!(hcl.contains("kernel_path = \"/boot/vmlinux\""));
        assert!(hcl.contains("ram_mib = 512"));
        assert!(hcl.contains("cpus = 2"));
    }
}
