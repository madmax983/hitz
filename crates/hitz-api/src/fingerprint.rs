//! Fingerprinting module for categorizing VM workload behavior.
//!
//! # Abstract
//! This module provides a way to quantize a continuous `MetricsSnapshot`
//! into a discrete identifier (a "fingerprint"). This allows for rapid clustering,
//! pattern matching, or caching of VMs exhibiting similar resource profiles.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FingerprintGenerator, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Assume a snapshot with 95% CPU, 40% RAM used, and minimal IO
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 95.0,
//!         per_core: vec![95.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1000,
//!         used_bytes: 400,
//!         free_bytes: 600,
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
//! let fingerprint = snap.generate_fingerprint();
//! assert_eq!(fingerprint.id, "FP-C9-R4-D0-N0");
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// A discrete identifier for a VM's resource usage profile.
///
/// This structure exists so we can map a VM's real-time resource utilization to a static string.
/// This acts as a caching key or classification label for automated scaling decisions.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::VmFingerprint;
///
/// let fp = VmFingerprint {
///     id: "FP-C9-R4-D0-N0".to_string(),
/// };
/// assert_eq!(fp.id, "FP-C9-R4-D0-N0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmFingerprint {
    /// The formatted fingerprint string (e.g., "FP-C9-R4-D0-N0").
    pub id: String,
}

/// Trait to generate a fingerprint.
///
/// This trait isolates the calculation of a fingerprint from the raw metrics structures.
/// It exists so that we can easily swap out the fingerprint generation logic or mock it in tests.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{FingerprintGenerator, VmFingerprint};
///
/// struct MockVm;
/// impl FingerprintGenerator for MockVm {
///     fn generate_fingerprint(&self) -> VmFingerprint {
///         VmFingerprint { id: "FP-MOCK".to_string() }
///     }
/// }
///
/// let vm = MockVm;
/// assert_eq!(vm.generate_fingerprint().id, "FP-MOCK");
/// ```
pub trait FingerprintGenerator {
    /// Generates a fingerprint based on current state.
    ///
    /// This function transforms continuous metrics arrays into a single, hashed string.
    /// It exists because we need a fast, comparable string representation of the VM's state,
    /// rather than re-computing CPU and memory percentages for every rule evaluation.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{FingerprintGenerator, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics {
    ///         total_pct: 95.0,
    ///         per_core: vec![95.0],
    ///         load_avg: [0.1, 0.1, 0.1],
    ///     },
    ///     memory: MemoryMetrics {
    ///         total_bytes: 1000,
    ///         used_bytes: 400,
    ///         free_bytes: 600,
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
    /// let fingerprint = snap.generate_fingerprint();
    /// assert_eq!(fingerprint.id, "FP-C9-R4-D0-N0");
    /// ```
    fn generate_fingerprint(&self) -> VmFingerprint;
}

impl FingerprintGenerator for MetricsSnapshot {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn generate_fingerprint(&self) -> VmFingerprint {
        // CPU Bucketing (0-10) based on percentage.
        // e.g., 95.0 / 10.0 = 9.5 -> 9. 100.0 / 10.0 = 10.
        let cpu_bucket = (self.cpu.total_pct / 10.0).clamp(0.0, 10.0) as u32;

        // RAM Bucketing (0-10) based on used / total percentage.
        let ram_bucket = if self.memory.total_bytes > 0 {
            let mem_pct = (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            (mem_pct / 10.0).clamp(0.0, 10.0) as u32
        } else {
            0
        };

        // Disk Bucketing (0-9) based on logarithmic magnitude of total read/write bytes.
        let total_disk_bytes: u64 = self
            .disks
            .iter()
            .map(|d| d.read_bytes + d.write_bytes)
            .sum();
        let disk_bucket = if total_disk_bytes > 0 {
            let log_bytes = (total_disk_bytes as f64).log10();
            log_bytes.clamp(0.0, 9.0) as u32
        } else {
            0
        };

        // Network Bucketing (0-9) based on logarithmic magnitude of total rx/tx bytes.
        let total_net_bytes: u64 = self.networks.iter().map(|n| n.rx_bytes + n.tx_bytes).sum();
        let net_bucket = if total_net_bytes > 0 {
            let log_bytes = (total_net_bytes as f64).log10();
            log_bytes.clamp(0.0, 9.0) as u32
        } else {
            0
        };

        VmFingerprint {
            id: format!("FP-C{cpu_bucket}-R{ram_bucket}-D{disk_bucket}-N{net_bucket}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics};

    fn dummy_snapshot(cpu: f32, mem_pct: u64, disk_bytes: u64, net_bytes: u64) -> MetricsSnapshot {
        // Need to ensure used_bytes calculation matches the (mem_pct / 10) logic closely.
        // If mem_pct is 40, used_mem / total_mem * 100 should be 40.0.
        let total_mem = 100_000_000;
        let used_mem = (total_mem * mem_pct) / 100;

        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: total_mem,
                used_bytes: used_mem,
                free_bytes: total_mem - used_mem,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".into(),
                reads_total: 0,
                writes_total: 0,
                read_bytes: disk_bytes,
                write_bytes: disk_bytes,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".into(),
                rx_bytes: net_bytes,
                tx_bytes: net_bytes,
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        }
    }

    #[test]
    fn test_zero_memory_total() {
        let mut snap = dummy_snapshot(10.0, 0, 0, 0);
        snap.memory.total_bytes = 0;
        let fp = snap.generate_fingerprint();
        assert_eq!(fp.id, "FP-C1-R0-D0-N0");
    }
    #[test]
    fn test_fingerprint_generation_idle() {
        // We use 11% RAM so (11.0 / 10.0) -> 1
        let snap = dummy_snapshot(5.0, 11, 0, 0);
        let fp = snap.generate_fingerprint();
        assert_eq!(fp.id, "FP-C0-R1-D0-N0");
    }

    #[test]
    fn test_fingerprint_generation_compute_heavy() {
        // We use 41% RAM so (41.0 / 10.0) -> 4
        let snap = dummy_snapshot(95.0, 41, 0, 0);
        let fp = snap.generate_fingerprint();
        assert_eq!(fp.id, "FP-C9-R4-D0-N0");
    }

    #[test]
    fn test_fingerprint_generation_io_heavy() {
        // We use 21% RAM so (21.0 / 10.0) -> 2
        let snap = dummy_snapshot(15.0, 21, 100_000, 1_000_000_000);
        let fp = snap.generate_fingerprint();
        assert_eq!(fp.id, "FP-C1-R2-D5-N9");
    }
}
