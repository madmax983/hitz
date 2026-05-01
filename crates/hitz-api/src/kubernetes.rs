//! Kubernetes exporter module.
//!
//! # Abstract
//! This module converts a `VmConfig` into a Kubernetes Pod manifest (YAML).
//! This allows for exporting micro-VM configurations to container orchestrators.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, GuestAgentMode, ToKubernetes};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 2,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let yaml = config.to_kubernetes("my-vm");
//! assert!(yaml.contains("kind: Pod"));
//! assert!(yaml.contains("memory: \"1024Mi\""));
//! ```

use crate::VmConfig;

/// Trait to convert a configuration to a Kubernetes Pod manifest.
pub trait ToKubernetes {
    /// Generates a Kubernetes Pod YAML manifest.
    fn to_kubernetes(&self, name: &str) -> String;
}

impl ToKubernetes for VmConfig {
    fn to_kubernetes(&self, name: &str) -> String {
        let mut yaml = String::new();
        yaml.push_str("apiVersion: v1\n");
        yaml.push_str("kind: Pod\n");
        yaml.push_str("metadata:\n");
        let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("  name: {name}\n"));
        yaml.push_str("  labels:\n");
        yaml.push_str("    app: hitz-vm\n");
        yaml.push_str("spec:\n");
        yaml.push_str("  containers:\n");
        let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("    - name: {name}\n"));
        // For a micro-VM, we'll represent the kernel as the image
        let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("      image: hitz/kernel:{}\n", self.kernel_path.display()));
        yaml.push_str("      resources:\n");
        yaml.push_str("        limits:\n");
        let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("          memory: \"{}Mi\"\n", self.ram_mib));
        let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("          cpu: \"{}\"\n", self.cpus));

        if !self.ports.is_empty() {
            yaml.push_str("      ports:\n");
            for port in &self.ports {
                let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("        - containerPort: {}\n", port.guest_port));
                let _ = std::fmt::Write::write_fmt(&mut yaml, format_args!("          hostPort: {}\n", port.host_port));
            }
        }
        yaml
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuestAgentMode, PortForward};
    use std::path::PathBuf;

    #[test]
    fn test_to_kubernetes_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux-custom"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 512,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        let yaml = config.to_kubernetes("test-pod");
        assert!(yaml.contains("name: test-pod"));
        assert!(yaml.contains("memory: \"512Mi\""));
        assert!(yaml.contains("cpu: \"4\""));
        assert!(yaml.contains("image: hitz/kernel:vmlinux-custom"));
    }

    #[test]
    fn test_to_kubernetes_with_ports() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 512,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![
                PortForward { host_port: 8080, guest_port: 80 },
                PortForward { host_port: 2222, guest_port: 22 },
            ],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        let yaml = config.to_kubernetes("web-pod");
        assert!(yaml.contains("containerPort: 80"));
        assert!(yaml.contains("hostPort: 8080"));
        assert!(yaml.contains("containerPort: 22"));
        assert!(yaml.contains("hostPort: 2222"));
    }
}
