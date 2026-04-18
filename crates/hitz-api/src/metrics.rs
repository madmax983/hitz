//! Telemetry and Metrics Structures.
//!
//! # Abstract
//! This module defines the wire-protocol structures used to convey real-time
//! telemetry data from the `hitz-guest-agent` to the host daemon over virtio-vsock.
//! The primary payload is the [`MetricsSnapshot`], which provides an absolute
//! point-in-time view of CPU, memory, disk, network, and process utilization.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // The daemon receives a fresh snapshot from the guest agent
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1_700_000_000_000,
//!     cpu: CpuMetrics {
//!         total_pct: 42.5,
//!         per_core: vec![40.0, 45.0],
//!         load_avg: [1.2, 0.8, 0.5],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 512 * 1024 * 1024,
//!         buffers_bytes: 0,
//!         cached_bytes: 0,
//!         swap_total: 0,
//!         swap_used: 0,
//!     },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // Analyze the data!
//! assert!(snap.cpu.total_pct > 0.0);
//! ```

use serde::{Deserialize, Serialize};

// ── Metrics wire protocol ────────────────────────────────────────────────────

/// On-demand metrics request sent from host to guest over vsock.
///
/// # Abstract
/// Defines the specific command sent by the hypervisor to the `hitz-guest-agent`
/// to pull a fresh batch of performance data.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::MetricsRequest;
///
/// let req = MetricsRequest::Snapshot;
/// assert_eq!(req, MetricsRequest::Snapshot);
/// ```
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
    /// Per-core CPU utilisation percentage (0.0–100.0).
    pub per_core: Vec<f32>,
    /// System load average over 1, 5, and 15 minutes.
    pub load_avg: [f32; 3],
}

/// Memory utilisation metrics.
///
/// Details total RAM and Swap, along with how much is actively used vs
/// free or cached.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::MemoryMetrics;
///
/// let mem = MemoryMetrics {
///     total_bytes: 104_857_600,
///     used_bytes: 52_428_800,
///     free_bytes: 52_428_800,
///     buffers_bytes: 0,
///     cached_bytes: 0,
///     swap_total: 0,
///     swap_used: 0,
/// };
///
/// assert_eq!(mem.total_bytes, mem.used_bytes + mem.free_bytes);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Total system RAM in bytes.
    pub total_bytes: u64,
    /// Actively used RAM in bytes.
    pub used_bytes: u64,
    /// Completely free RAM in bytes.
    pub free_bytes: u64,
    /// RAM used for block device buffers in bytes.
    pub buffers_bytes: u64,
    /// RAM used for page cache in bytes.
    pub cached_bytes: u64,
    /// Total swap space in bytes.
    pub swap_total: u64,
    /// Used swap space in bytes.
    pub swap_used: u64,
}

/// I/O metrics for a single block device.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::DiskMetrics;
///
/// let disk = DiskMetrics {
///     name: "vda".to_string(),
///     read_bytes: 1024,
///     write_bytes: 2048,
///     reads_total: 10,
///     writes_total: 20,
/// };
///
/// assert_eq!(disk.name, "vda");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskMetrics {
    /// Block device name (e.g. `vda`).
    pub name: String,
    /// Total bytes read since boot.
    pub read_bytes: u64,
    /// Total bytes written since boot.
    pub write_bytes: u64,
    /// Total read operations since boot.
    pub reads_total: u64,
    /// Total write operations since boot.
    pub writes_total: u64,
}

/// Network metrics for a single interface.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetMetrics;
///
/// let net = NetMetrics {
///     interface: "eth0".to_string(),
///     rx_bytes: 5000,
///     tx_bytes: 1000,
///     rx_packets: 50,
///     tx_packets: 10,
///     rx_errors: 0,
///     tx_errors: 0,
/// };
///
/// assert_eq!(net.interface, "eth0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetMetrics {
    /// Network interface name (e.g. `eth0`).
    pub interface: String,
    /// Total bytes received since boot.
    pub rx_bytes: u64,
    /// Total bytes transmitted since boot.
    pub tx_bytes: u64,
    /// Total packets received since boot.
    pub rx_packets: u64,
    /// Total packets transmitted since boot.
    pub tx_packets: u64,
    /// Total receive errors since boot.
    pub rx_errors: u64,
    /// Total transmit errors since boot.
    pub tx_errors: u64,
}

/// Process-level metrics.
///
/// Captures the resource usage of a specific process running in the guest.
/// Usually collected for the top N processes by CPU usage.
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
///     rss_bytes: 4096,
///     state: 'S',
/// };
///
/// assert_eq!(proc.pid, 1);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcMetrics {
    /// Process ID (PID).
    pub pid: u32,
    /// Process name / command.
    pub name: String,
    /// CPU utilisation percentage (0.0–100.0).
    pub cpu_pct: f32,
    /// Resident Set Size (RSS) memory in bytes.
    pub rss_bytes: u64,
    /// Process state character (e.g., 'R' for running, 'S' for sleeping).
    pub state: char,
}
