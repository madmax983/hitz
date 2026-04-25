//! Calculating differences and rates between telemetry snapshots.
//!
//! # Abstract
//! This module provides the tools necessary to calculate rates of change
//! over time (per second) by comparing two telemetry snapshots. This is essential
//! for converting raw, monotonically increasing counters (like bytes read from disk)
//! into actionable metrics (like bytes/sec).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
//! use hitz_api::{CalculateDiff, MetricsDiff};
//!
//! // We create an initial snapshot at time = 1000ms
//! let snap1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // We create a second snapshot 2 seconds later (time = 3000ms)
//! let snap2 = MetricsSnapshot {
//!     timestamp_ms: 3000,
//!     cpu: CpuMetrics { total_pct: 15.0, per_core: vec![15.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // Calculate the difference!
//! let diff = snap2.diff(&snap1).expect("snap2 is newer than snap1");
//!
//! // The diff accurately reflects the 2-second elapsed time
//! assert_eq!(diff.elapsed_secs, 2.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Calculated rates of change per second between two [`MetricsSnapshot`]s.
///
/// # Abstract
/// This struct holds the unified record of calculated rates. It takes the absolute
/// counter values from a `MetricsSnapshot` (like total bytes read) and normalizes
/// them into a "per second" rate.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::MetricsDiff;
///
/// let diff = MetricsDiff {
///     elapsed_secs: 1.0,
///     disks: vec![],
///     networks: vec![],
/// };
/// assert_eq!(diff.elapsed_secs, 1.0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsDiff {
    /// Time elapsed between the two snapshots in seconds.
    pub elapsed_secs: f64,
    /// Disk rates of change (per second).
    pub disks: Vec<DiskRate>,
    /// Network rates of change (per second).
    pub networks: Vec<NetRate>,
}

/// Per-disk I/O rates (per second).
///
/// # Abstract
/// Represents the rate of read and write operations for a single disk over time.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::DiskRate;
///
/// let rate = DiskRate {
///     name: "vda".to_string(),
///     reads_per_sec: 10.0,
///     writes_per_sec: 5.0,
///     read_bytes_per_sec: 1024.0,
///     write_bytes_per_sec: 512.0,
/// };
/// assert_eq!(rate.name, "vda");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskRate {
    /// Device name.
    pub name: String,
    /// Reads per second.
    pub reads_per_sec: f64,
    /// Writes per second.
    pub writes_per_sec: f64,
    /// Bytes read per second.
    pub read_bytes_per_sec: f64,
    /// Bytes written per second.
    pub write_bytes_per_sec: f64,
}

/// Per-network-interface I/O rates (per second).
///
/// # Abstract
/// Represents the rate of packet transmission and reception for a single network interface.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::NetRate;
///
/// let rate = NetRate {
///     interface: "eth0".to_string(),
///     rx_bytes_per_sec: 1024.0,
///     tx_bytes_per_sec: 512.0,
///     rx_packets_per_sec: 10.0,
///     tx_packets_per_sec: 5.0,
/// };
/// assert_eq!(rate.interface, "eth0");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetRate {
    /// Interface name.
    pub interface: String,
    /// Bytes received per second.
    pub rx_bytes_per_sec: f64,
    /// Bytes transmitted per second.
    pub tx_bytes_per_sec: f64,
    /// Packets received per second.
    pub rx_packets_per_sec: f64,
    /// Packets transmitted per second.
    pub tx_packets_per_sec: f64,
}

/// A trait for types that can calculate the difference between themselves.
///
/// # Abstract
/// This trait establishes a standardized interface for objects that represent
/// an absolute value at a specific point in time (like a snapshot of counters)
/// to produce a relative rate of change when compared to an older snapshot.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_api::CalculateDiff;
///
/// struct MyCounter { time: u64, count: u64 }
/// struct MyRate { ops_per_sec: f64 }
///
/// impl CalculateDiff for MyCounter {
///     type Diff = MyRate;
///
///     fn diff(&self, previous: &Self) -> Option<Self::Diff> {
///         if self.time <= previous.time { return None; }
///         let elapsed = (self.time - previous.time) as f64;
///         let ops = (self.count.saturating_sub(previous.count)) as f64;
///         Some(MyRate { ops_per_sec: ops / elapsed })
///     }
/// }
///
/// let c1 = MyCounter { time: 1, count: 10 };
/// let c2 = MyCounter { time: 3, count: 50 }; // 40 ops over 2 seconds
///
/// let rate = c2.diff(&c1).unwrap();
/// assert_eq!(rate.ops_per_sec, 20.0);
/// ```
pub trait CalculateDiff {
    /// The resulting difference type.
    type Diff;

    /// Calculates the difference between `self` and `previous`.
    ///
    /// # Details
    /// Implementations should return the difference relative to the elapsed
    /// time between the two objects.
    ///
    /// Returns `None` if the duration between `self` and `previous` is zero
    /// or negative (i.e., `self` is not strictly newer than `previous`).
    fn diff(&self, previous: &Self) -> Option<Self::Diff>;
}

impl CalculateDiff for MetricsSnapshot {
    type Diff = MetricsDiff;

    #[allow(clippy::cast_precision_loss)]
    fn diff(&self, previous: &Self) -> Option<Self::Diff> {
        if self.timestamp_ms <= previous.timestamp_ms {
            return None;
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;

        // Pre-allocate to avoid dynamic heap reallocations during iteration
        let mut disks = Vec::with_capacity(self.disks.len());
        disks.extend(self.disks.iter().filter_map(|current_disk| {
            let prev_disk = previous
                .disks
                .iter()
                .find(|d| d.name == current_disk.name)?;
            Some(DiskRate {
                name: current_disk.name.clone(),
                reads_per_sec: (current_disk
                    .reads_total
                    .saturating_sub(prev_disk.reads_total)) as f64
                    / elapsed_secs,
                writes_per_sec: (current_disk
                    .writes_total
                    .saturating_sub(prev_disk.writes_total)) as f64
                    / elapsed_secs,
                read_bytes_per_sec: (current_disk.read_bytes.saturating_sub(prev_disk.read_bytes))
                    as f64
                    / elapsed_secs,
                write_bytes_per_sec: (current_disk
                    .write_bytes
                    .saturating_sub(prev_disk.write_bytes))
                    as f64
                    / elapsed_secs,
            })
        }));

        // Pre-allocate to avoid dynamic heap reallocations during iteration
        let mut networks = Vec::with_capacity(self.networks.len());
        networks.extend(self.networks.iter().filter_map(|current_net| {
            let prev_net = previous
                .networks
                .iter()
                .find(|n| n.interface == current_net.interface)?;
            Some(NetRate {
                interface: current_net.interface.clone(),
                rx_bytes_per_sec: (current_net.rx_bytes.saturating_sub(prev_net.rx_bytes)) as f64
                    / elapsed_secs,
                tx_bytes_per_sec: (current_net.tx_bytes.saturating_sub(prev_net.tx_bytes)) as f64
                    / elapsed_secs,
                rx_packets_per_sec: (current_net.rx_packets.saturating_sub(prev_net.rx_packets))
                    as f64
                    / elapsed_secs,
                tx_packets_per_sec: (current_net.tx_packets.saturating_sub(prev_net.tx_packets))
                    as f64
                    / elapsed_secs,
            })
        }));

        Some(MetricsDiff {
            elapsed_secs,
            disks,
            networks,
        })
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics};

    fn dummy_snapshot(time_ms: u64, disk_io: u64, net_io: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: time_ms,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 100,
                free_bytes: 900,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".to_string(),
                reads_total: disk_io,
                writes_total: disk_io,
                read_bytes: disk_io * 1024,
                write_bytes: disk_io * 1024,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: net_io * 1024,
                tx_bytes: net_io * 1024,
                rx_packets: net_io,
                tx_packets: net_io,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        }
    }

    #[test]
    fn should_return_none_when_diffing_older_or_equal_snapshot() {
        let t1 = dummy_snapshot(1000, 100, 50);
        let t2 = dummy_snapshot(500, 50, 25);
        let t3 = dummy_snapshot(1000, 100, 50);

        assert!(t2.diff(&t1).is_none());
        assert!(t3.diff(&t1).is_none());
    }

    #[test]
    fn test_metrics_diff_calculation() {
        // Create an older snapshot
        let t1 = dummy_snapshot(1000, 100, 50);

        // Create a newer snapshot 2.5 seconds later
        let t2 = dummy_snapshot(3500, 350, 150);

        // Calculate diff: (350 - 100) = 250 ops over 2.5 seconds = 100 ops/sec
        // For net IO: (150 - 50) = 100 packets over 2.5 seconds = 40 packets/sec
        let diff = t2.diff(&t1).expect("Diff should be Some");

        assert_eq!(diff.elapsed_secs, 2.5);

        assert_eq!(diff.disks.len(), 1);
        assert_eq!(diff.disks[0].name, "vda");
        assert_eq!(diff.disks[0].reads_per_sec, 100.0);
        assert_eq!(diff.disks[0].writes_per_sec, 100.0);
        assert_eq!(diff.disks[0].read_bytes_per_sec, 102400.0);
        assert_eq!(diff.disks[0].write_bytes_per_sec, 102400.0);

        assert_eq!(diff.networks.len(), 1);
        assert_eq!(diff.networks[0].interface, "eth0");
        assert_eq!(diff.networks[0].rx_packets_per_sec, 40.0);
        assert_eq!(diff.networks[0].tx_packets_per_sec, 40.0);
        assert_eq!(diff.networks[0].rx_bytes_per_sec, 40960.0);
        assert_eq!(diff.networks[0].tx_bytes_per_sec, 40960.0);
    }

    #[test]
    fn should_handle_missing_devices_in_diff() {
        // Older snapshot has no disks and no networks
        let mut t1 = dummy_snapshot(1000, 0, 0);
        t1.disks.clear();
        t1.networks.clear();

        // Newer snapshot has disks and networks
        let mut t2 = dummy_snapshot(2000, 100, 50);
        t2.disks[0].name = "vda".to_string();
        t2.networks[0].interface = "eth0".to_string();

        let diff = t2.diff(&t1).expect("Diff should be Some");
        assert_eq!(diff.elapsed_secs, 1.0);

        // Since t1 didn't have them, diff should not include them
        assert!(
            diff.disks.is_empty(),
            "Disks should be empty when missing in previous snapshot"
        );
        assert!(
            diff.networks.is_empty(),
            "Networks should be empty when missing in previous snapshot"
        );
    }

    #[test]
    fn should_prevent_panic_on_counter_reset() {
        // Older snapshot has higher counter values (e.g., guest rebooted or agent restarted)
        let mut t1 = dummy_snapshot(1000, 500, 500);
        t1.disks[0].reads_total = 500;
        t1.networks[0].rx_bytes = 500 * 1024;

        // Newer snapshot has lower counter values
        let mut t2 = dummy_snapshot(2000, 100, 100);
        t2.disks[0].reads_total = 100;
        t2.networks[0].rx_bytes = 100 * 1024;

        // Ensure diff() doesn't panic on underflow
        let diff = t2.diff(&t1).expect("Diff should be Some");
        assert_eq!(diff.elapsed_secs, 1.0);

        // Subtraction should saturate to 0
        assert_eq!(diff.disks[0].reads_per_sec, 0.0);
        assert_eq!(diff.networks[0].rx_bytes_per_sec, 0.0);
    }
}
