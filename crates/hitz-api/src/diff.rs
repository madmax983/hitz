//! Calculating differences and rates between telemetry snapshots.

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Calculated rates of change per second between two [`MetricsSnapshot`]s.
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
pub trait CalculateDiff {
    /// The resulting difference type.
    type Diff;

    /// Calculates the difference between `self` and `previous`.
    ///
    /// Returns `None` if the duration between the two is zero or negative.
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

        let mut disks = Vec::new();
        for current_disk in &self.disks {
            if let Some(prev_disk) = previous.disks.iter().find(|d| d.name == current_disk.name) {
                disks.push(DiskRate {
                    name: current_disk.name.clone(),
                    reads_per_sec: (current_disk
                        .reads_total
                        .saturating_sub(prev_disk.reads_total))
                        as f64
                        / elapsed_secs,
                    writes_per_sec: (current_disk
                        .writes_total
                        .saturating_sub(prev_disk.writes_total))
                        as f64
                        / elapsed_secs,
                    read_bytes_per_sec: (current_disk
                        .read_bytes
                        .saturating_sub(prev_disk.read_bytes))
                        as f64
                        / elapsed_secs,
                    write_bytes_per_sec: (current_disk
                        .write_bytes
                        .saturating_sub(prev_disk.write_bytes))
                        as f64
                        / elapsed_secs,
                });
            }
        }

        let mut networks = Vec::new();
        for current_net in &self.networks {
            if let Some(prev_net) = previous
                .networks
                .iter()
                .find(|n| n.interface == current_net.interface)
            {
                networks.push(NetRate {
                    interface: current_net.interface.clone(),
                    rx_bytes_per_sec: (current_net.rx_bytes.saturating_sub(prev_net.rx_bytes))
                        as f64
                        / elapsed_secs,
                    tx_bytes_per_sec: (current_net.tx_bytes.saturating_sub(prev_net.tx_bytes))
                        as f64
                        / elapsed_secs,
                    rx_packets_per_sec: (current_net.rx_packets.saturating_sub(prev_net.rx_packets))
                        as f64
                        / elapsed_secs,
                    tx_packets_per_sec: (current_net.tx_packets.saturating_sub(prev_net.tx_packets))
                        as f64
                        / elapsed_secs,
                });
            }
        }

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
}
