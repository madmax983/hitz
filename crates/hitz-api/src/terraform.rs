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
///
/// # Abstract
/// Implementing this trait allows domain structures (like [`VmConfig`]) to be
/// translated into a declarative representation suitable for infrastructure-as-code deployments.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{VmConfig, ToTerraform, GuestAgentMode};
/// use std::path::PathBuf;
///
/// let config = VmConfig {
///     kernel_path: PathBuf::from("/boot/vmlinux"),
///     initramfs_path: None,
///     disk_path: None,
///     ram_mib: 1024,
///     cpus: 4,
///     cmdline: None,
///     net: None,
///     ports: vec![],
///     guest_cid: 4,
///     guest_agent: GuestAgentMode::Auto,
/// };
///
/// let hcl = config.to_terraform("my_app_vm");
/// assert!(hcl.starts_with("resource \"hitz_vm\" \"my_app_vm\""));
/// ```
pub trait ToTerraform {
    /// Returns the Terraform HCL representation as a String.
    ///
    /// # Abstract
    /// Generates the HCL block mapping this instance's specific configuration variables
    /// to the properties defined by the target Terraform provider resource schema.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{VmConfig, ToTerraform, GuestAgentMode};
    /// use std::path::PathBuf;
    ///
    /// let config = VmConfig {
    ///     kernel_path: PathBuf::from("/boot/vmlinux"),
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
    /// // Generate the HCL configuration snippet for "worker_node"
    /// let hcl_snippet = config.to_terraform("worker_node");
    /// assert!(hcl_snippet.contains("worker_node"));
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
    use std::path::PathBuf;
    use crate::GuestAgentMode;

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
