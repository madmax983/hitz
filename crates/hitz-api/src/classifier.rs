//! Classifier module for determining workload type.
//!
//! # Abstract
//! This module connects the absolute state of a system (`MetricsSnapshot`) with
//! the relative rate of change (`MetricsDiff`) to categorize the current behavior
//! of the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{WorkloadClass, WorkloadClassifier, MetricsSnapshot, MetricsDiff};
//!
//! // Assume we have a snapshot showing 95% CPU usage and a diff showing low I/O
//! // let snap: MetricsSnapshot = ...;
//! // let diff: MetricsDiff = ...;
//! // let class = snap.classify_workload(&diff);
//! // assert_eq!(class, WorkloadClass::ComputeBound);
//! ```

use crate::{MetricsDiff, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The categorized type of workload running on the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadClass {
    /// The VM is heavily utilizing the CPU.
    ComputeBound,
    /// The VM is heavily utilizing RAM (high usage, possibly low free space).
    MemoryBound,
    /// The VM is performing a high volume of disk or network I/O.
    IoHeavy,
    /// The VM is mostly idle.
    Idle,
}

/// Trait for objects that can classify their workload.
///
/// # Abstract
/// This trait provides a standardized interface for categorizing the current behavior
/// of a VM (e.g., [`WorkloadClass::ComputeBound`]) based on its state and rate of change.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{WorkloadClass, WorkloadClassifier, MetricsSnapshot, MetricsDiff, CpuMetrics, MemoryMetrics};
///
/// let snap = MetricsSnapshot {
///     timestamp_ms: 1000,
///     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![95.0], load_avg: [1.0, 1.0, 1.0] },
///     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
///     disks: vec![],
///     networks: vec![],
///     processes: vec![],
/// };
///
/// let diff = MetricsDiff { elapsed_secs: 1.0, disks: vec![], networks: vec![] };
/// let class = snap.classify_workload(&diff);
/// assert_eq!(class, WorkloadClass::ComputeBound);
/// ```
pub trait WorkloadClassifier {
    /// Classifies the workload based on the current state and a rate-of-change diff.
    fn classify_workload(&self, diff: &MetricsDiff) -> WorkloadClass;
}

impl WorkloadClassifier for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn classify_workload(&self, diff: &MetricsDiff) -> WorkloadClass {
        if self.cpu.total_pct > 80.0 {
            return WorkloadClass::ComputeBound;
        }

        if self.memory.total_bytes > 0 {
            let memory_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            if memory_pct > 80.0 {
                return WorkloadClass::MemoryBound;
            }
        }

        let mut disk_io_heavy = false;
        for disk in &diff.disks {
            if disk.reads_per_sec > 500.0 || disk.writes_per_sec > 500.0 {
                disk_io_heavy = true;
            }
        }

        let mut net_io_heavy = false;
        for net in &diff.networks {
            if net.rx_bytes_per_sec > 10_000_000.0 || net.tx_bytes_per_sec > 10_000_000.0 {
                net_io_heavy = true;
            }
        }

        if disk_io_heavy || net_io_heavy {
            return WorkloadClass::IoHeavy;
        }

        WorkloadClass::Idle
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, DiskRate, MemoryMetrics, NetMetrics, NetRate};

    fn base_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 900 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".into(),
                reads_total: 0,
                writes_total: 0,
                read_bytes: 0,
                write_bytes: 0,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".into(),
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        }
    }

    fn base_diff() -> MetricsDiff {
        MetricsDiff {
            elapsed_secs: 1.0,
            disks: vec![DiskRate {
                name: "vda".into(),
                reads_per_sec: 10.0,
                writes_per_sec: 10.0,
                read_bytes_per_sec: 1024.0,
                write_bytes_per_sec: 1024.0,
            }],
            networks: vec![NetRate {
                interface: "eth0".into(),
                rx_bytes_per_sec: 1024.0,
                tx_bytes_per_sec: 1024.0,
                rx_packets_per_sec: 10.0,
                tx_packets_per_sec: 10.0,
            }],
        }
    }

    #[test]
    fn test_compute_bound() {
        let mut snap = base_snapshot();
        snap.cpu.total_pct = 85.0; // High CPU
        let diff = base_diff();

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::ComputeBound);
    }

    #[test]
    fn test_memory_bound() {
        let mut snap = base_snapshot();
        snap.memory.used_bytes = 850 * 1024 * 1024; // High memory usage (> 80%)
        let diff = base_diff();

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::MemoryBound);
    }

    #[test]
    fn test_io_heavy_disk_writes() {
        let snap = base_snapshot();
        let mut diff = base_diff();
        diff.disks[0].writes_per_sec = 600.0; // High disk IO writes

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::IoHeavy);
    }

    #[test]
    fn test_io_heavy_net_tx() {
        let snap = base_snapshot();
        let mut diff = base_diff();
        diff.networks[0].tx_bytes_per_sec = 50_000_000.0; // High net IO tx

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::IoHeavy);
    }
    #[test]
    fn test_io_heavy_disk() {
        let snap = base_snapshot();
        let mut diff = base_diff();
        diff.disks[0].reads_per_sec = 600.0; // High disk IO

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::IoHeavy);
    }

    #[test]
    fn test_io_heavy_net() {
        let snap = base_snapshot();
        let mut diff = base_diff();
        diff.networks[0].rx_bytes_per_sec = 50_000_000.0; // High net IO

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::IoHeavy);
    }

    #[test]
    fn test_idle() {
        let snap = base_snapshot();
        let diff = base_diff();

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::Idle);
    }

    #[test]
    fn should_handle_zero_memory_total_gracefully() {
        let mut snap = base_snapshot();
        snap.memory.total_bytes = 0; // Avoid division by zero
        snap.memory.used_bytes = 100;

        let diff = base_diff();

        assert_eq!(snap.classify_workload(&diff), WorkloadClass::Idle);
    }
}
