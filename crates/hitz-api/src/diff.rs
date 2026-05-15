//! Metrics difference calculator.
//!
//! # Abstract
//! Calculates the rate of change (e.g., ops/sec, bytes/sec) between two
//! `MetricsSnapshot` instances.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CalculateDiff, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let t1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let mut t2 = t1.clone();
//! t2.timestamp_ms = 2000;
//!
//! let diff = t2.diff(&t1).expect("should succeed");
//! assert_eq!(diff.elapsed_secs, 1.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The computed difference between two metrics snapshots.
///
/// # Abstract
/// This struct holds the calculated rates (per second) for discrete events
/// like disk reads or network packets. It is essential for generating Prometheus
/// gauges that reflect current throughput rather than just raw counters.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{MetricsDiff, DiskRate};
///
/// let diff = MetricsDiff {
///     elapsed_secs: 1.0,
///     disks: vec![DiskRate {
///         name: "vda".to_string(),
///         reads_per_sec: 100.0,
///         writes_per_sec: 50.0,
///         read_bytes_per_sec: 4096.0,
///         write_bytes_per_sec: 2048.0,
///     }],
///     networks: vec![],
/// };
///
/// assert_eq!(diff.disks[0].reads_per_sec, 100.0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsDiff {
    /// The actual elapsed time between the two snapshots, in seconds.
    pub elapsed_secs: f64,
    /// Rate of change for all active disks.
    pub disks: Vec<DiskRate>,
    /// Rate of change for all active network interfaces.
    pub networks: Vec<NetRate>,
}

/// Disk throughput and IOPs rates.
///
/// # Abstract
/// Represents the per-second activity of a single block device.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::DiskRate;
///
/// let rate = DiskRate {
///     name: "vdb".to_string(),
///     reads_per_sec: 10.0,
///     writes_per_sec: 0.0,
///     read_bytes_per_sec: 512.0,
///     write_bytes_per_sec: 0.0,
/// };
///
/// assert_eq!(rate.name, "vdb");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskRate {
    /// Device name (e.g., "vda").
    pub name: String,
    /// Read operations per second.
    pub reads_per_sec: f64,
    /// Write operations per second.
    pub writes_per_sec: f64,
    /// Read bytes per second.
    pub read_bytes_per_sec: f64,
    /// Write bytes per second.
    pub write_bytes_per_sec: f64,
}

/// Network throughput and packet rates.
///
/// # Abstract
/// Represents the per-second activity of a single network interface.
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
///
/// assert_eq!(rate.interface, "eth0");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetRate {
    /// Interface name (e.g., "eth0").
    pub interface: String,
    /// Received bytes per second.
    pub rx_bytes_per_sec: f64,
    /// Transmitted bytes per second.
    pub tx_bytes_per_sec: f64,
    /// Received packets per second.
    pub rx_packets_per_sec: f64,
    /// Transmitted packets per second.
    pub tx_packets_per_sec: f64,
}

/// Trait to calculate the difference between two temporal states.
///
/// # Abstract
/// Anything that implements this trait can be compared against a previous version
/// of itself to generate a delta or rate of change.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::CalculateDiff;
///
/// struct MyCounter {
///     time: u64,
///     count: u64,
/// }
///
/// struct MyRate {
///     ops_per_sec: f64,
/// }
///
/// impl CalculateDiff for MyCounter {
///     type Diff = MyRate;
///     fn diff(&self, prev: &Self) -> Option<Self::Diff> {
///         if self.time <= prev.time { return None; }
///         let elapsed = (self.time - prev.time) as f64;
///         let ops = (self.count.saturating_sub(prev.count)) as f64;
///         Some(MyRate { ops_per_sec: ops / elapsed })
///     }
/// }
///
/// let c1 = MyCounter { time: 1, count: 10 };
/// let c2 = MyCounter { time: 3, count: 50 }; // 40 ops over 2 seconds
///
/// let rate = c2.diff(&c1).expect("should succeed");
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
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{CalculateDiff, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let mut t1 = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
    ///     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
    ///     disks: vec![],
    ///     networks: vec![],
    ///     processes: vec![],
    /// };
    ///
    /// let mut t2 = t1.clone();
    /// t2.timestamp_ms = 2000;
    ///
    /// let diff = t2.diff(&t1).expect("should succeed");
    /// assert_eq!(diff.elapsed_secs, 1.0);
    /// ```
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
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::expect_used,
    clippy::cast_precision_loss
)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics};

    pub fn dummy_snapshot(time_ms: u64, disk_io: u64, net_io: u64) -> MetricsSnapshot {
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
    fn test_diff_timestamps_table_driven() {
        struct TestCase {
            t1_ms: u64,
            t2_ms: u64,
            expect_none: bool,
        }

        let test_cases = vec![
            TestCase {
                t1_ms: 1000,
                t2_ms: 2000,
                expect_none: false,
            },
            TestCase {
                t1_ms: 2000,
                t2_ms: 1000,
                expect_none: true,
            },
            TestCase {
                t1_ms: 1000,
                t2_ms: 1000,
                expect_none: true,
            },
            TestCase {
                t1_ms: 0,
                t2_ms: 1,
                expect_none: false,
            },
            TestCase {
                t1_ms: 1,
                t2_ms: 0,
                expect_none: true,
            },
        ];

        for case in test_cases {
            let t1 = dummy_snapshot(case.t1_ms, 100, 50);
            let t2 = dummy_snapshot(case.t2_ms, 200, 100);

            let diff_result = t2.diff(&t1);

            if case.expect_none {
                assert!(
                    diff_result.is_none(),
                    "Expected None for t1={} t2={}",
                    case.t1_ms,
                    case.t2_ms
                );
            } else {
                assert!(
                    diff_result.is_some(),
                    "Expected Some for t1={} t2={}",
                    case.t1_ms,
                    case.t2_ms
                );

                let diff = diff_result.expect("should be some");
                let expected_elapsed = (case.t2_ms - case.t1_ms) as f64 / 1000.0;
                assert_eq!(diff.elapsed_secs, expected_elapsed);
            }
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
    fn should_handle_new_devices_in_snapshot() {
        // Older snapshot has disks and networks
        let t1 = dummy_snapshot(1000, 10, 10);

        // Newer snapshot has same devices plus new ones
        let mut t2 = dummy_snapshot(2000, 20, 20);
        t2.disks.push(DiskMetrics {
            name: "vdb".to_string(),
            reads_total: 50,
            writes_total: 50,
            read_bytes: 50 * 1024,
            write_bytes: 50 * 1024,
        });
        t2.networks.push(NetMetrics {
            interface: "eth1".to_string(),
            rx_bytes: 50 * 1024,
            tx_bytes: 50 * 1024,
            rx_packets: 50,
            tx_packets: 50,
            rx_errors: 0,
            tx_errors: 0,
        });

        let diff = t2.diff(&t1).expect("Diff should be Some");
        assert_eq!(diff.elapsed_secs, 1.0);

        // Diff should only include devices present in both
        assert_eq!(diff.disks.len(), 1);
        assert_eq!(diff.disks[0].name, "vda");
        assert_eq!(diff.networks.len(), 1);
        assert_eq!(diff.networks[0].interface, "eth0");
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
