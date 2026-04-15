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

#[cfg(feature = "health_check")]
/// Health assessment module for evaluating system telemetry.
mod health;
#[cfg(feature = "health_check")]
pub use health::*;

#[cfg(feature = "simulator")]
/// Simulator module for generating synthetic telemetry streams.
mod simulator;
#[cfg(feature = "simulator")]
pub use simulator::*;

#[cfg(feature = "diff")]
/// Diff module for calculating rates of change between telemetry snapshots.
mod diff;
#[cfg(feature = "diff")]
pub use diff::*;

#[cfg(feature = "classifier")]
/// Classifier module for determining workload type.
pub mod classifier;
#[cfg(feature = "classifier")]
pub use classifier::*;

#[cfg(feature = "prometheus")]
/// Prometheus module for converting metrics to Prometheus text format.
pub mod prometheus;
#[cfg(feature = "prometheus")]
pub use prometheus::*;

#[cfg(feature = "recommender")]
/// Recommender module for generating scaling actions based on health and workload.
pub mod recommender;
#[cfg(feature = "recommender")]
pub use recommender::*;

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
/// This structure defines how the micro-VM connects to the host network.
/// By default, Hitz sets up a point-to-point interface (like `WinTun` on Windows
/// or `TAP` on Linux) to allow network traffic between the host and the guest.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetConfig;
///
/// let net = NetConfig {
///     mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
///     host_ip: "192.168.100.1/24".to_string(),
///     guest_ip: "192.168.100.2/24".to_string(),
///     adapter_name: Some("hitz-dev-01".to_string()),
/// };
///
/// assert_eq!(net.host_ip, "192.168.100.1/24");
/// assert_eq!(net.guest_ip, "192.168.100.2/24");
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
/// ## Examples
///
/// ```rust
/// use hitz_api::PortForward;
///
/// // Forward host port 8080 to guest port 80
/// let rule = PortForward {
///     host_port: 8080,
///     guest_port: 80,
/// };
///
/// assert_eq!(rule.host_port, 8080);
/// assert_eq!(rule.guest_port, 80);
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
/// Hitz supports injecting a lightweight agent into the guest environment to
/// extract process and resource usage telemetry. By default, it auto-injects
/// the bundled `hitz-guest-agent`.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::GuestAgentMode;
/// use std::path::PathBuf;
///
/// // The default mode uses the built-in guest agent (if available).
/// let default_mode = GuestAgentMode::default();
/// assert_eq!(default_mode, GuestAgentMode::Auto);
///
/// // You can also supply a custom static binary for testing or
/// // specialized data collection.
/// let custom_mode = GuestAgentMode::Custom(PathBuf::from("/usr/local/bin/my-agent"));
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
///
/// Captures overall and per-core CPU usage, alongside system load averages.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::CpuMetrics;
///
/// let cpu = CpuMetrics {
///     total_pct: 45.2,
///     per_core: vec![40.0, 50.4],
///     load_avg: [1.5, 1.2, 0.9],
/// };
///
/// assert_eq!(cpu.per_core.len(), 2);
/// assert!(cpu.total_pct > 0.0);
/// ```
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
///
/// Tracks how RAM is being consumed inside the guest, including buffers
/// and page cache usage which are often reclaimable.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::MemoryMetrics;
///
/// let mem = MemoryMetrics {
///     total_bytes: 1024 * 1024 * 1024, // 1 GB
///     used_bytes: 512 * 1024 * 1024,   // 512 MB
///     free_bytes: 256 * 1024 * 1024,   // 256 MB
///     buffers_bytes: 64 * 1024 * 1024, // 64 MB
///     cached_bytes: 192 * 1024 * 1024, // 192 MB
///     swap_total: 0,
///     swap_used: 0,
/// };
///
/// assert_eq!(mem.total_bytes, mem.used_bytes + mem.free_bytes + mem.buffers_bytes + mem.cached_bytes);
/// ```
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
///
/// Provides insights into block device activity, such as reads, writes,
/// and total bytes transferred.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::DiskMetrics;
///
/// let disk = DiskMetrics {
///     name: "vda".to_string(),
///     reads_total: 1500,
///     writes_total: 300,
///     read_bytes: 4096 * 1500,
///     write_bytes: 4096 * 300,
/// };
///
/// assert_eq!(disk.name, "vda");
/// assert!(disk.reads_total > 0);
/// ```
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
///
/// Tracks packets and bytes transmitted and received on a specific interface,
/// along with any errors encountered.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetMetrics;
///
/// let net = NetMetrics {
///     interface: "eth0".to_string(),
///     rx_bytes: 1048576,
///     tx_bytes: 524288,
///     rx_packets: 1024,
///     tx_packets: 512,
///     rx_errors: 0,
///     tx_errors: 0,
/// };
///
/// assert_eq!(net.interface, "eth0");
/// assert_eq!(net.rx_errors, 0);
/// ```
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
///
/// Describes the resource usage of a single process running inside the guest.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::ProcMetrics;
///
/// let proc = ProcMetrics {
///     pid: 1,
///     name: "systemd".to_string(),
///     cpu_pct: 0.1,
///     rss_bytes: 8 * 1024 * 1024,
///     state: 'S', // Sleeping
/// };
///
/// assert_eq!(proc.pid, 1);
/// assert_eq!(proc.state, 'S');
/// ```
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
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// let config = VmConfig {
///     kernel_path: PathBuf::from("/boot/vmlinux"),
///     initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
///     disk_path: None,
///     ram_mib: 512,
///     cpus: 2,
///     cmdline: Some("console=ttyS0 quiet".to_string()),
///     net: None,
///     ports: vec![],
///     guest_cid: 4,
///     guest_agent: GuestAgentMode::Auto,
/// };
///
/// assert_eq!(config.ram_mib, 512);
/// assert_eq!(config.cpus, 2);
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
/// This enum represents the discrete commands that can be issued to
/// the daemon to control a micro-VM's lifecycle over the REST API.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmAction;
///
/// let action = VmAction::Start;
///
/// match action {
///     VmAction::Start => println!("Starting the VM..."),
///     VmAction::Stop => println!("Stopping the VM..."),
///     VmAction::Restart => println!("Restarting the VM..."),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmAction {
    /// Start (resume) a created or stopped VM.
    Start,
    /// Stop (pause/shutdown) a running VM.
    Stop,
    /// Restart a running VM by stopping it and then starting it.
    Restart,
}

/// Current lifecycle state of a VM.
///
/// This tracks the operational status of a micro-VM from its initial
/// creation, through active execution, until it is gracefully stopped
/// or encounters a fatal error.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmState;
///
/// let state = VmState::Running;
///
/// assert!(matches!(state, VmState::Running));
///
/// if state == VmState::Failed {
///     println!("The VM encountered a fatal error.");
/// }
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
///
/// # Abstract
/// This type is the payload sent from the CLI to the daemon's REST API when
/// asking to instantiate a new micro-VM. It wraps the core [`VmConfig`] so the
/// daemon knows exactly how to build the partition.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{CreateVmRequest, VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// // Create the request object containing the blueprint for our new VM
/// let req = CreateVmRequest {
///     config: VmConfig {
///         kernel_path: PathBuf::from("/path/to/vmlinux"),
///         initramfs_path: None,
///         disk_path: None,
///         ram_mib: 256,
///         cpus: 1,
///         cmdline: None,
///         net: None,
///         ports: vec![],
///         guest_agent: GuestAgentMode::Auto,
///         guest_cid: 3,
///     },
/// };
///
/// // Verify we packed 1 CPU for the journey
/// assert_eq!(req.config.cpus, 1);
/// ```
///
/// # Details
/// This request must be serialized into JSON and sent via a `PUT` request to
/// the daemon at `/vms/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    /// Configuration blueprint for the VM to create.
    pub config: VmConfig,
}

/// Request to perform an action on an existing VM.
///
/// # Abstract
/// This type is used to command the daemon to change the lifecycle state of a VM.
/// Instead of separate endpoints for starting, stopping, or restarting, a single
/// `POST /vms/{id}/action` endpoint accepts this payload.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{ActionVmRequest, VmAction};
///
/// // Prepare a command to gracefully halt the VM
/// let req = ActionVmRequest {
///     action: VmAction::Stop,
/// };
///
/// // Confirm the intent is to stop
/// assert_eq!(req.action, VmAction::Stop);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionVmRequest {
    /// The discrete action to perform (e.g., [`VmAction::Start`] or [`VmAction::Stop`]).
    pub action: VmAction,
}

/// Request to clone an existing VM.
///
/// # Abstract
/// Used to duplicate an existing VM configuration into a new instance with a
/// specified destination ID. This is particularly useful for spawning multiple
/// identical workers.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::CloneVmRequest;
///
/// // Tell the daemon to create a carbon-copy VM named 'cloned-vm-01'
/// let req = CloneVmRequest {
///     dest_id: "cloned-vm-01".to_string(),
/// };
///
/// assert_eq!(req.dest_id, "cloned-vm-01");
/// ```
///
/// # Details
/// Sent via `POST /vms/{src_id}/clone`. The source VM can be in any [`VmState`],
/// but the newly cloned VM will always start in the [`VmState::Created`] state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVmRequest {
    /// The new unique identifier for the cloned VM instance.
    pub dest_id: String,
}

/// Information about a VM instance returned by the API.
///
/// # Abstract
/// Summarizes the identity, current state, and configuration of a micro-VM.
/// The daemon responds with this payload when querying a specific VM or listing
/// all VMs.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{VmInfo, VmState, VmConfig, GuestAgentMode};
/// use std::path::PathBuf;
///
/// // The daemon returns the vital statistics of the running VM
/// let info = VmInfo {
///     id: "my-vm".to_string(),
///     state: VmState::Running,
///     config: VmConfig {
///         kernel_path: PathBuf::from("/vmlinux"),
///         initramfs_path: None,
///         disk_path: None,
///         ram_mib: 512,
///         cpus: 2,
///         cmdline: None,
///         net: None,
///         ports: vec![],
///         guest_agent: GuestAgentMode::Auto,
///         guest_cid: 3,
///     },
///     exit_reason: None,
/// };
///
/// // We can check if it's still alive
/// assert_eq!(info.id, "my-vm");
/// assert_eq!(info.state, VmState::Running);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    /// The unique identifier assigned to this VM.
    pub id: String,
    /// The current lifecycle status (e.g., [`VmState::Running`]).
    pub state: VmState,
    /// The exact configuration the VM was created with.
    pub config: VmConfig,
    /// If the VM exited or failed, a human-readable explanation of why.
    pub exit_reason: Option<String>,
}

/// Error response from the API.
///
/// # Abstract
/// When an API call fails (like trying to start a VM that doesn't exist), the
/// daemon returns this structured error detailing what went wrong. It's the
/// first line of defense for debugging CLI interactions.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::ApiError;
///
/// // The daemon encountered a user error and politely explains why
/// let err = ApiError {
///     message: "VM not found: my-vm".to_string(),
/// };
///
/// assert_eq!(err.message, "VM not found: my-vm");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// A human-readable, descriptive error message explaining the failure.
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
        for action in [VmAction::Start, VmAction::Stop, VmAction::Restart] {
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
