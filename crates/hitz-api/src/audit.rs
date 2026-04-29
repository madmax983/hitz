//! Security auditing module for evaluating configuration risks.
//!
//! # Abstract
//! This module analyzes a `VmConfig` to provide actionable warnings
//! about potentially insecure settings (e.g. disabled agents, permissive
//! cmdlines) before deployment.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{SecurityAuditor, AuditResult, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: Some("root=/dev/vda rw init=/bin/bash".to_string()),
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Disabled,
//! };
//!
//! let audit = config.run_audit();
//! assert_eq!(audit.is_secure, false);
//! assert!(audit.warnings.len() >= 2);
//! ```

use crate::{GuestAgentMode, VmConfig};
use serde::{Deserialize, Serialize};

/// The result of a security audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditResult {
    /// Is the configuration considered secure?
    pub is_secure: bool,
    /// A list of specific security warnings.
    pub warnings: Vec<String>,
}

/// Trait for objects that can be security audited.
pub trait SecurityAuditor {
    /// Runs a security audit and returns the results.
    fn run_audit(&self) -> AuditResult;
}

impl SecurityAuditor for VmConfig {
    fn run_audit(&self) -> AuditResult {
        let mut warnings = Vec::new();

        if matches!(self.guest_agent, GuestAgentMode::Disabled) {
            warnings.push(
                "Guest agent is disabled. Telemetry and graceful shutdown will be unavailable."
                    .to_string(),
            );
        }

        if let Some(cmd) = &self.cmdline {
            if cmd.contains("init=/bin/bash") || cmd.contains("init=/bin/sh") {
                warnings.push(
                    "Insecure init binary in cmdline. Bypasses normal startup procedures."
                        .to_string(),
                );
            }
            if cmd.contains("single") || cmd.contains("emergency") {
                warnings.push("Booting into single-user or emergency mode.".to_string());
            }
        }

        for port in &self.ports {
            if port.host_port < 1024 {
                warnings.push(format!(
                    "Forwarding to privileged host port: {}",
                    port.host_port
                ));
            }
        }

        AuditResult {
            is_secure: warnings.is_empty(),
            warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PortForward;
    use std::path::PathBuf;

    fn base_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    #[test]
    fn test_secure_config() {
        let cfg = base_config();
        let audit = cfg.run_audit();
        assert!(audit.is_secure);
        assert!(audit.warnings.is_empty());
    }

    #[test]
    fn test_disabled_guest_agent() {
        let mut cfg = base_config();
        cfg.guest_agent = GuestAgentMode::Disabled;
        let audit = cfg.run_audit();
        assert!(!audit.is_secure);
        assert!(
            audit
                .warnings
                .iter()
                .any(|w| w.contains("Guest agent is disabled"))
        );
    }

    #[test]
    fn test_insecure_cmdline() {
        let mut cfg = base_config();
        cfg.cmdline = Some("init=/bin/bash".to_string());
        let audit = cfg.run_audit();
        assert!(!audit.is_secure);
        assert!(
            audit
                .warnings
                .iter()
                .any(|w| w.contains("Insecure init binary in cmdline"))
        );
    }

    #[test]
    fn test_privileged_ports() {
        let mut cfg = base_config();
        cfg.ports = vec![PortForward {
            host_port: 80,
            guest_port: 8080,
        }];
        let audit = cfg.run_audit();
        assert!(!audit.is_secure);
        assert!(
            audit
                .warnings
                .iter()
                .any(|w| w.contains("Forwarding to privileged host port"))
        );
    }
}
