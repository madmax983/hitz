use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::net::{NetConfig, PortForward};
use crate::guest::GuestAgentMode;
use crate::{DEFAULT_CMDLINE, DEFAULT_GUEST_CID, DEFAULT_CPUS};

const fn default_cpus() -> u32 {
    DEFAULT_CPUS
}

const fn default_guest_cid() -> u32 {
    DEFAULT_GUEST_CID
}

/// Definition of a micro-VM.
///
/// Contains all the specifications needed to launch a VM, including its
/// boot images, resource limits, and device attachments.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{VmConfig, DEFAULT_CMDLINE, DEFAULT_RAM_MIB, DEFAULT_CPUS, DEFAULT_GUEST_CID, GuestAgentMode};
/// use std::path::PathBuf;
///
/// let config = VmConfig {
///     kernel_path: PathBuf::from("/boot/vmlinux"),
///     initramfs_path: Some(PathBuf::from("/boot/initrd.cpio")),
///     disk_path: None,
///     ram_mib: 512,
///     cpus: 2,
///     cmdline: Some("console=ttyS0 quiet".to_string()),
///     net: None,
///     ports: vec![],
///     guest_cid: DEFAULT_GUEST_CID,
///     guest_agent: GuestAgentMode::default(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmConfig {
    /// Path to the kernel ELF binary (vmlinux).
    pub kernel_path: PathBuf,
    /// Optional initramfs (cpio archive) path.
    pub initramfs_path: Option<PathBuf>,
    /// Optional disk image path for virtio-blk.
    pub disk_path: Option<PathBuf>,
    /// Guest RAM in MiB (default: 256).
    pub ram_mib: u32,
    /// Number of virtual CPUs (default: 1).
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    /// Custom kernel command line (default: [`DEFAULT_CMDLINE`]).
    pub cmdline: Option<String>,
    /// Optional network configuration for virtio-net.
    pub net: Option<NetConfig>,
    /// TCP port forwards. Empty = no forwarding.
    ///
    /// Each rule opens a `TcpListener` on `0.0.0.0:host_port` and proxies
    /// connections to `guest_ip:guest_port`. Ignored if `net` is `None`.
    #[serde(default)]
    pub ports: Vec<PortForward>,
    /// Guest CID for vsock communication (default: 3).
    #[serde(default = "default_guest_cid")]
    pub guest_cid: u32,
    /// Guest agent injection strategy.
    #[serde(default)]
    pub guest_agent: GuestAgentMode,
}

impl VmConfig {
    /// Returns the effective command line: custom if set, otherwise the default.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hitz_api::{VmConfig, DEFAULT_CMDLINE, DEFAULT_RAM_MIB, DEFAULT_CPUS, DEFAULT_GUEST_CID, GuestAgentMode};
    /// use std::path::PathBuf;
    ///
    /// let mut config = VmConfig {
    ///     kernel_path: PathBuf::from("vmlinux"),
    ///     initramfs_path: None,
    ///     disk_path: None,
    ///     ram_mib: DEFAULT_RAM_MIB,
    ///     cpus: DEFAULT_CPUS,
    ///     cmdline: None,
    ///     net: None,
    ///     ports: vec![],
    ///     guest_cid: DEFAULT_GUEST_CID,
    ///     guest_agent: GuestAgentMode::Auto,
    /// };
    ///
    /// assert_eq!(config.effective_cmdline(), DEFAULT_CMDLINE);
    ///
    /// config.cmdline = Some("root=/dev/vda rw".to_string());
    /// assert_eq!(config.effective_cmdline(), "root=/dev/vda rw");
    /// ```
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        self.cmdline.as_deref().unwrap_or(DEFAULT_CMDLINE)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{DEFAULT_RAM_MIB, DEFAULT_HOST_IP, DEFAULT_GUEST_IP};

    fn minimal_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    #[test]
    fn vm_config_guest_agent_serde_default() {
        // Old configs without guest_agent field should deserialize as Auto.
        let json = r#"{"kernel_path":"/k","ram_mib":256,"cpus":1}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert!(matches!(cfg.guest_agent, GuestAgentMode::Auto));
    }
    #[test]
    fn vm_config_guest_cid_serde_default() {
        let json = r#"{"kernel_path":"/k","ram_mib":256,"cpus":1}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.guest_cid, DEFAULT_GUEST_CID);
    }
    #[test]
    fn effective_cmdline_default() {
        let cfg = minimal_config();
        assert_eq!(cfg.effective_cmdline(), DEFAULT_CMDLINE);
    }
    #[test]
    fn effective_cmdline_custom() {
        let cfg = VmConfig {
            cmdline: Some("root=/dev/vda rw".into()),
            ..minimal_config()
        };
        assert_eq!(cfg.effective_cmdline(), "root=/dev/vda rw");
    }
    #[test]
    fn default_ram_mib_value() {
        assert_eq!(DEFAULT_RAM_MIB, 256);
    }
    #[test]
    fn serde_roundtrip() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
            disk_path: Some(PathBuf::from("/images/root.img")),
            ram_mib: 512,
            cpus: 1,
            cmdline: Some("console=ttyS0".into()),
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.kernel_path, cfg.kernel_path);
        assert_eq!(restored.initramfs_path, cfg.initramfs_path);
        assert_eq!(restored.disk_path, cfg.disk_path);
        assert_eq!(restored.ram_mib, cfg.ram_mib);
        assert_eq!(restored.cmdline, cfg.cmdline);
    }
    #[test]
    fn serde_roundtrip_minimal() {
        let cfg = minimal_config();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.kernel_path, cfg.kernel_path);
        assert!(restored.initramfs_path.is_none());
        assert!(restored.disk_path.is_none());
        assert_eq!(restored.ram_mib, DEFAULT_RAM_MIB);
        assert!(restored.cmdline.is_none());
    }
    #[test]
    fn vm_config_with_net_serde() {
        let cfg = VmConfig {
            net: Some(NetConfig {
                mac: None,
                host_ip: DEFAULT_HOST_IP.into(),
                guest_ip: DEFAULT_GUEST_IP.into(),
                adapter_name: None,
            }),
            ..minimal_config()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.net.is_some());
    }
    #[test]
    fn serde_cpus_default() {
        let json = r#"{"kernel_path":"vmlinux","initramfs_path":null,"disk_path":null,"ram_mib":256,"cmdline":null,"net":null}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.cpus, DEFAULT_CPUS);
    }
    #[test]
    fn serde_cpus_explicit() {
        let cfg = VmConfig {
            cpus: 4,
            ..minimal_config()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.cpus, 4);
    }
    #[test]
    fn vm_config_ports_serde_default() {
        // Old config JSON without a "ports" field should deserialize to empty vec.
        let json = r#"{"kernel_path":"vmlinux","initramfs_path":null,"disk_path":null,"ram_mib":256,"cmdline":null,"net":null}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert!(cfg.ports.is_empty());
    }
    #[test]
    fn vm_config_ports_roundtrip() {
        let cfg = VmConfig {
            ports: vec![
                PortForward {
                    host_port: 2222,
                    guest_port: 22,
                },
                PortForward {
                    host_port: 8080,
                    guest_port: 80,
                },
            ],
            ..minimal_config()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, cfg);
    }
}
