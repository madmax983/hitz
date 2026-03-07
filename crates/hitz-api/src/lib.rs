//! REST API types for Hitz.
//!
//! Request/response types and the `VmAction` enum. No I/O — pure data.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default kernel command line for Linux direct boot.
pub const DEFAULT_CMDLINE: &str = "console=ttyS0 earlyprintk=serial";

/// Default guest RAM in MiB (256 MiB).
pub const DEFAULT_RAM_MIB: u32 = 256;

/// Default number of virtual CPUs.
pub const DEFAULT_CPUS: u32 = 1;

/// Default host IP for guest networking.
pub const DEFAULT_HOST_IP: &str = "192.168.100.1/24";
/// Default guest IP for guest networking.
pub const DEFAULT_GUEST_IP: &str = "192.168.100.2/24";

/// Network configuration for a VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetConfig {
    /// Guest MAC address (e.g. "AA:BB:CC:DD:EE:FF"). Random if `None`.
    pub mac: Option<String>,
    /// Host-side IP address with prefix (e.g. "192.168.100.1/24").
    pub host_ip: String,
    /// Guest-side IP address with prefix (e.g. "192.168.100.2/24").
    pub guest_ip: String,
    /// `WinTun` adapter name. Defaults to "hitz-{vm_id}" if `None`.
    pub adapter_name: Option<String>,
}

/// A single TCP port forward rule: `host_port` on the host forwards to
/// `guest_port` inside the VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForward {
    /// Port to listen on the host (e.g. `2222`).
    pub host_port: u16,
    /// Port to connect to in the guest (e.g. `22`).
    pub guest_port: u16,
}

const fn default_cpus() -> u32 {
    DEFAULT_CPUS
}

/// VM configuration — everything needed to boot a micro-VM.
///
/// Serializable for the future daemon REST API (Phase 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl VmConfig {
    /// Returns the effective command line: custom if set, otherwise the default.
    #[must_use]
    pub fn effective_cmdline(&self) -> &str {
        self.cmdline.as_deref().unwrap_or(DEFAULT_CMDLINE)
    }
}

/// Action that can be performed on a running VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmAction {
    /// Start (resume) a created or stopped VM.
    Start,
    /// Stop (pause/shutdown) a running VM.
    Stop,
}

/// Current lifecycle state of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmState {
    /// VM has been created but not yet started.
    Created,
    /// VM is actively running.
    Running,
    /// VM has been stopped (gracefully or by request).
    Stopped,
    /// VM encountered a fatal error.
    Failed,
}

/// Request to create a new VM with the given configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    /// Configuration for the VM to create.
    pub config: VmConfig,
}

/// Request to perform an action on an existing VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionVmRequest {
    /// The action to perform.
    pub action: VmAction,
}

/// Information about a VM instance returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    /// Unique identifier for this VM.
    pub id: String,
    /// Current lifecycle state.
    pub state: VmState,
    /// Configuration the VM was created with.
    pub config: VmConfig,
    /// If the VM exited or failed, the reason why.
    pub exit_reason: Option<String>,
}

/// Error response from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Human-readable error message.
    pub message: String,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn effective_cmdline_default() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
        };
        assert_eq!(cfg.effective_cmdline(), DEFAULT_CMDLINE);
    }

    #[test]
    fn effective_cmdline_custom() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: Some("root=/dev/vda rw".into()),
            net: None,
            ports: vec![],
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
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.kernel_path, cfg.kernel_path);
        assert!(restored.initramfs_path.is_none());
        assert!(restored.disk_path.is_none());
        assert_eq!(restored.ram_mib, DEFAULT_RAM_MIB);
        assert!(restored.cmdline.is_none());
    }

    #[test]
    fn vm_action_serde_roundtrip() {
        for action in [VmAction::Start, VmAction::Stop] {
            let json = serde_json::to_string(&action).expect("serialize");
            let restored: VmAction = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, action);
        }
    }

    #[test]
    fn vm_state_serde_roundtrip() {
        for state in [
            VmState::Created,
            VmState::Running,
            VmState::Stopped,
            VmState::Failed,
        ] {
            let json = serde_json::to_string(&state).expect("serialize");
            let restored: VmState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, state);
        }
    }

    #[test]
    fn vm_info_serde_roundtrip() {
        let info = VmInfo {
            id: "vm-001".into(),
            state: VmState::Running,
            config: VmConfig {
                kernel_path: PathBuf::from("/boot/vmlinux"),
                initramfs_path: None,
                disk_path: Some(PathBuf::from("/images/root.img")),
                ram_mib: 512,
                cpus: 1,
                cmdline: Some("console=ttyS0".into()),
                net: None,
                ports: vec![],
            },
            exit_reason: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: VmInfo = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.id, info.id);
        assert_eq!(restored.state, info.state);
        assert_eq!(restored.config.ram_mib, info.config.ram_mib);
        assert!(restored.exit_reason.is_none());
    }

    #[test]
    fn create_vm_request_serde() {
        let req = CreateVmRequest {
            config: VmConfig {
                kernel_path: PathBuf::from("vmlinux"),
                initramfs_path: None,
                disk_path: None,
                ram_mib: DEFAULT_RAM_MIB,
                cpus: 1,
                cmdline: None,
                net: None,
                ports: vec![],
            },
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CreateVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.config.kernel_path, req.config.kernel_path);
        assert_eq!(restored.config.ram_mib, req.config.ram_mib);
    }

    #[test]
    fn action_vm_request_serde() {
        let req = ActionVmRequest {
            action: VmAction::Stop,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: ActionVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.action, VmAction::Stop);
    }

    #[test]
    fn api_error_serde() {
        let err = ApiError {
            message: "VM not found".into(),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let restored: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.message, err.message);
    }

    #[test]
    fn net_config_serde_roundtrip() {
        let cfg = NetConfig {
            mac: Some("AA:BB:CC:DD:EE:FF".into()),
            host_ip: DEFAULT_HOST_IP.into(),
            guest_ip: DEFAULT_GUEST_IP.into(),
            adapter_name: Some("hitz-test".into()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: NetConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.host_ip, cfg.host_ip);
        assert_eq!(restored.guest_ip, cfg.guest_ip);
        assert_eq!(restored.mac, cfg.mac);
    }

    #[test]
    fn vm_config_with_net_serde() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: Some(NetConfig {
                mac: None,
                host_ip: DEFAULT_HOST_IP.into(),
                guest_ip: DEFAULT_GUEST_IP.into(),
                adapter_name: None,
            }),
            ports: vec![],
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
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cmdline: None,
            net: None,
            cpus: 4,
            ports: vec![],
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.cpus, 4);
    }

    #[test]
    fn port_forward_serde_roundtrip() {
        let pf = PortForward {
            host_port: 2222,
            guest_port: 22,
        };
        let json = serde_json::to_string(&pf).expect("serialize");
        let restored: PortForward = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, pf);
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
            kernel_path: "vmlinux".into(),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 256,
            cpus: 1,
            cmdline: None,
            net: None,
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
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.ports, cfg.ports);
    }
}
