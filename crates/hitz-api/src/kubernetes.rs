//! Kubernetes Pod Manifest Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` into a Kubernetes Pod YAML manifest, allowing users to
//! export their micro-VM configurations for deployment in Kubernetes clusters
//! (e.g., via specialized Kubelet implementations or Kata Containers).
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, ToKubernetes, GuestAgentMode};
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
//! let yaml = config.to_kubernetes("my-vm");
//! assert!(yaml.contains("apiVersion: v1"));
//! assert!(yaml.contains("kind: Pod"));
//! assert!(yaml.contains("name: my-vm"));
//! assert!(yaml.contains("memory: \"1024Mi\""));
//! assert!(yaml.contains("cpu: \"4\""));
//! ```

use crate::VmConfig;
use std::fmt::Write as _;

/// Trait to export structures to Kubernetes YAML representation.
pub trait ToKubernetes {
    /// Returns the Kubernetes YAML representation as a String.
    fn to_kubernetes(&self, pod_name: &str) -> String;
}

impl ToKubernetes for VmConfig {
    fn to_kubernetes(&self, pod_name: &str) -> String {
        let mut yaml = String::with_capacity(512);
        let _ = writeln!(yaml, "apiVersion: v1");
        let _ = writeln!(yaml, "kind: Pod");
        let _ = writeln!(yaml, "metadata:");
        let _ = writeln!(yaml, "  name: {pod_name}");
        let _ = writeln!(yaml, "spec:");
        let _ = writeln!(yaml, "  containers:");
        let _ = writeln!(yaml, "    - name: {pod_name}-container");
        let _ = writeln!(yaml, "      image: hitz/micro-vm:latest");
        let _ = writeln!(yaml, "      resources:");
        let _ = writeln!(yaml, "        requests:");
        let _ = writeln!(yaml, "          memory: \"{}Mi\"", self.ram_mib);
        let _ = writeln!(yaml, "          cpu: \"{}\"", self.cpus);
        let _ = writeln!(yaml, "        limits:");
        let _ = writeln!(yaml, "          memory: \"{}Mi\"", self.ram_mib);
        let _ = writeln!(yaml, "          cpu: \"{}\"", self.cpus);

        if let Some(cmdline) = &self.cmdline {
            let _ = writeln!(yaml, "      command: [\"/init\"]");
            let _ = writeln!(yaml, "      args: [\"{cmdline}\"]");
        }

        yaml
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_to_kubernetes_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 512,
            cpus: 2,
            cmdline: Some("quiet".to_string()),
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let yaml = config.to_kubernetes("test-vm");
        assert!(yaml.contains("apiVersion: v1"));
        assert!(yaml.contains("kind: Pod"));
        assert!(yaml.contains("name: test-vm"));
        assert!(yaml.contains("memory: \"512Mi\""));
        assert!(yaml.contains("cpu: \"2\""));
        assert!(yaml.contains("args: [\"quiet\"]"));
    }
}
