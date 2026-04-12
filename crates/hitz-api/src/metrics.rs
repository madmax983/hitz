use serde::{Deserialize, Serialize};

/// On-demand metrics request sent from host to guest over vsock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricsRequest {
    /// Request a full resource snapshot.
    Snapshot,
}

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
/// assert_eq!(snap.cpu.total_pct, 12.5);
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
///     load_avg: [1.2, 0.8, 0.5],
/// };
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
/// Models the standard Linux `free` command output.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::MemoryMetrics;
///
/// let mem = MemoryMetrics {
///     total_bytes: 1024 * 1024,
///     used_bytes: 512 * 1024,
///     free_bytes: 512 * 1024,
///     buffers_bytes: 0,
///     cached_bytes: 0,
///     swap_total: 0,
///     swap_used: 0,
/// };
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
    /// Swap space in use.
    pub swap_used: u64,
}

/// Disk I/O metrics for a single block device.
///
/// Corresponds to the fields parsed from `/proc/diskstats`.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::DiskMetrics;
///
/// let disk = DiskMetrics {
///     name: "vda".to_string(),
///     reads_total: 150,
///     writes_total: 200,
///     read_bytes: 10240,
///     write_bytes: 20480,
/// };
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

/// Network I/O metrics for a single interface.
///
/// Corresponds to the fields parsed from `/proc/net/dev`.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetMetrics;
///
/// let net = NetMetrics {
///     interface: "eth0".to_string(),
///     rx_bytes: 5000,
///     tx_bytes: 3000,
///     rx_packets: 50,
///     tx_packets: 30,
///     rx_errors: 0,
///     tx_errors: 0,
/// };
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

/// Metrics for a single process.
///
/// Used to identify "hot" processes inside the guest.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::ProcMetrics;
///
/// let proc = ProcMetrics {
///     pid: 1234,
///     name: "nginx".to_string(),
///     cpu_pct: 12.5,
///     rss_bytes: 50 * 1024 * 1024,
///     state: 'S',
/// };
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn metrics_request_serde_roundtrip() {
        let req = MetricsRequest::Snapshot;
        let json = serde_json::to_string(&req).expect("serialize");
        let decoded: MetricsRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, req);
    }
}
