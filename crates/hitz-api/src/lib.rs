//! REST API types for Hitz.
//!
//! This crate serves as the central vocabulary for the Hitz micro-VM manager.
//! It defines the pure data structures used for communicating between the
//! command-line interface, the background daemon, and the guest agents running
//! inside the micro-VMs.
//!
//! By keeping all I/O out of this crate, we ensure these types can be compiled
//! for any target (including the Linux `musl` guest agents) and serialized
//! effortlessly across network boundaries.
//!
//! ## Core Concepts
//!
//! * **Configuration**: [`VmConfig`] describes everything needed to boot a VM.
//! * **Lifecycle**: [`VmState`] and [`VmAction`] track and control the VM's status.
//! * **Metrics**: [`MetricsSnapshot`] and its sub-types (like [`CpuMetrics`]) form the
//!   wire protocol over vsock for extracting real-time telemetry from the guest.

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

/// Default guest CID for virtio-vsock (host=2, first guest=3).
pub const DEFAULT_GUEST_CID: u32 = 3;

/// Metrics port on which the guest agent listens and the host connects.
pub const VSOCK_METRICS_PORT: u32 = 52355;

/// Host CID as defined by the virtio-vsock spec.
pub const VMADDR_CID_HOST: u32 = 2;

/// Network configuration for a VM.
///
/// This specifies how the VM integrates with the host's networking stack, allowing
/// the micro-VM to communicate with the outside world.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetConfig;
///
/// let config = NetConfig {
///     mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
///     host_ip: "192.168.100.1/24".to_string(),
///     guest_ip: "192.168.100.2/24".to_string(),
///     adapter_name: Some("hitz-test".to_string()),
/// };
///
/// assert_eq!(config.host_ip, "192.168.100.1/24");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
///
/// This structure acts as a mapping allowing external host traffic to reach
/// internal micro-VM services securely.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::PortForward;
///
/// let port_forward = PortForward {
///     host_port: 8080,
///     guest_port: 80,
/// };
///
/// assert_eq!(port_forward.host_port, 8080);
/// assert_eq!(port_forward.guest_port, 80);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForward {
    /// Port to listen on the host (e.g. `2222`).
    pub host_port: u16,
    /// Port to connect to in the guest (e.g. `22`).
    pub guest_port: u16,
}

// ── Guest agent mode ─────────────────────────────────────────────────────────

/// Controls whether and which guest metrics agent is injected into the initramfs.
///
/// The guest agent handles internal VM health telemetry. This mode determines
/// if the built-in, a custom, or no agent is provided to the VM during boot.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::GuestAgentMode;
/// use std::path::PathBuf;
///
/// let auto_agent = GuestAgentMode::Auto;
/// let custom_agent = GuestAgentMode::Custom(PathBuf::from("/usr/local/bin/agent"));
/// let disabled_agent = GuestAgentMode::Disabled;
///
/// assert_eq!(auto_agent, GuestAgentMode::default());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", content = "path", rename_all = "lowercase")]
pub enum GuestAgentMode {
    /// Automatically inject the built-in agent (default).
    #[default]
    Auto,
    /// Inject a user-supplied agent binary instead of the built-in one.
    Custom(PathBuf),
    /// Do not inject any agent.
    Disabled,
}

// ── Metrics wire protocol ────────────────────────────────────────────────────

/// On-demand metrics request sent from host to guest over vsock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricsRequest {
    /// Request a full resource snapshot.
    Snapshot,
}

// ── Metrics snapshot ─────────────────────────────────────────────────────────

/// Full guest resource snapshot, describing exactly what the VM is doing at
/// a given millisecond.
///
/// This is the response sent by the guest agent when the host requests
/// telemetry via [`MetricsRequest::Snapshot`]. It contains aggregated CPU,
/// memory, disk, network, and top process data.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
///
/// let snap = MetricsSnapshot {
///     timestamp_ms: 1_700_000_000_000,
///     cpu: CpuMetrics {
///         total_pct: 12.5,
///         per_core: vec![10.0, 15.0],
///         load_avg: [0.5, 0.4, 0.3],
///     },
///     memory: MemoryMetrics {
///         total_bytes: 256 * 1024 * 1024,
///         used_bytes: 100 * 1024 * 1024,
///         free_bytes: 156 * 1024 * 1024,
///         buffers_bytes: 0,
///         cached_bytes: 0,
///         swap_total: 0,
///         swap_used: 0,
///     },
///     disks: vec![],
///     networks: vec![],
///     processes: vec![],
/// };
///
/// assert_eq!(snap.memory.free_bytes, 156 * 1024 * 1024);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// CPU utilisation metrics.
    pub cpu: CpuMetrics,
    /// Memory utilisation metrics.
    pub memory: MemoryMetrics,
    /// Per-disk I/O metrics.
    pub disks: Vec<DiskMetrics>,
    /// Per-network-interface metrics.
    pub networks: Vec<NetMetrics>,
    /// Top processes by CPU usage (up to 10).
    pub processes: Vec<ProcMetrics>,
}

/// CPU utilisation metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Overall CPU utilisation percentage (0.0–100.0).
    pub total_pct: f32,
    /// Per-core utilisation percentages.
    pub per_core: Vec<f32>,
    /// Load averages: 1-minute, 5-minute, 15-minute.
    pub load_avg: [f32; 3],
}

/// Memory utilisation metrics (all in bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Total physical memory.
    pub total_bytes: u64,
    /// Memory in use.
    pub used_bytes: u64,
    /// Free (unallocated) memory.
    pub free_bytes: u64,
    /// Memory used for I/O buffers.
    pub buffers_bytes: u64,
    /// Memory used for page cache.
    pub cached_bytes: u64,
    /// Total swap space.
    pub swap_total: u64,
    /// Swap space currently in use.
    pub swap_used: u64,
}

/// Per-disk I/O metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskMetrics {
    /// Device name (e.g. "vda").
    pub name: String,
    /// Total completed read operations.
    pub reads_total: u64,
    /// Total completed write operations.
    pub writes_total: u64,
    /// Total bytes read.
    pub read_bytes: u64,
    /// Total bytes written.
    pub write_bytes: u64,
}

/// Per-network-interface metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetMetrics {
    /// Interface name (e.g. "eth0").
    pub interface: String,
    /// Total bytes received.
    pub rx_bytes: u64,
    /// Total bytes transmitted.
    pub tx_bytes: u64,
    /// Total packets received.
    pub rx_packets: u64,
    /// Total packets transmitted.
    pub tx_packets: u64,
    /// Total receive errors.
    pub rx_errors: u64,
    /// Total transmit errors.
    pub tx_errors: u64,
}

/// Per-process metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcMetrics {
    /// Process ID.
    pub pid: u32,
    /// Process name.
    pub name: String,
    /// CPU utilisation percentage.
    pub cpu_pct: f32,
    /// Resident set size in bytes.
    pub rss_bytes: u64,
    /// Process state character (e.g. 'R', 'S', 'Z').
    pub state: char,
}

// ── VmConfig helpers ──────────────────────────────────────────────────────────

const fn default_cpus() -> u32 {
    DEFAULT_CPUS
}

const fn default_guest_cid() -> u32 {
    DEFAULT_GUEST_CID
}

/// VM configuration — everything needed to boot a micro-VM.
///
/// Serializable for the future daemon REST API (Phase 6).
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
    /// Virtio-vsock guest CID. Must be unique per running VM. Default: 3.
    #[serde(default = "default_guest_cid")]
    pub guest_cid: u32,
    /// Guest metrics agent injection mode.
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

/// Action that can be performed on a running VM.
///
/// Use this enum to request state changes, such as starting a stopped VM
/// or gracefully stopping a running one.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmAction;
///
/// let start_action = VmAction::Start;
/// let stop_action = VmAction::Stop;
///
/// assert_ne!(start_action, stop_action);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmAction {
    /// Start (resume) a created or stopped VM.
    Start,
    /// Stop (pause/shutdown) a running VM.
    Stop,
}

/// Current lifecycle state of a VM.
///
/// Indicates whether the VM is booting, running normally, stopped gracefully,
/// or has encountered a fatal error.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmState;
///
/// let running = VmState::Running;
/// let failed = VmState::Failed;
///
/// assert_ne!(running, failed);
/// ```
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

/// Request to clone an existing VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVmRequest {
    /// The ID for the new cloned VM.
    pub dest_id: String,
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

    // ── Helper to build a minimal VmConfig ───────────────────────────────────

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

    // ── Phase 12 new tests ────────────────────────────────────────────────────

    #[test]
    fn metrics_snapshot_msgpack_roundtrip() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1_700_000_000_000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 156 * 1024 * 1024,
                buffers_bytes: 10 * 1024 * 1024,
                cached_bytes: 30 * 1024 * 1024,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let encoded = rmp_serde::to_vec(&snap).expect("encode");
        let decoded: MetricsSnapshot = rmp_serde::from_slice(&encoded).expect("decode");
        assert!((decoded.cpu.total_pct - 12.5).abs() < f32::EPSILON);
        assert_eq!(decoded.memory.total_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn guest_agent_mode_default_is_auto() {
        let mode: GuestAgentMode = GuestAgentMode::default();
        assert!(matches!(mode, GuestAgentMode::Auto));
    }

    #[test]
    fn guest_agent_mode_custom_serde_roundtrip() {
        let mode = GuestAgentMode::Custom(std::path::PathBuf::from("/usr/local/bin/my-agent"));
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }

    #[test]
    fn guest_agent_mode_disabled_serde_roundtrip() {
        let mode = GuestAgentMode::Disabled;
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }

    #[test]
    fn metrics_request_serde_roundtrip() {
        let req = MetricsRequest::Snapshot;
        let json = serde_json::to_string(&req).expect("serialize");
        let decoded: MetricsRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, req);
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

    // ── Existing tests (updated for new VmConfig fields) ─────────────────────

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
                guest_cid: DEFAULT_GUEST_CID,
                guest_agent: GuestAgentMode::Auto,
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
            config: minimal_config(),
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
    fn clone_vm_request_serde() {
        let req = CloneVmRequest {
            dest_id: "new-vm-123".into(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CloneVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.dest_id, req.dest_id);
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
