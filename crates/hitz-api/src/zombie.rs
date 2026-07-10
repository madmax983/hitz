//! Zombie VM detection module.
//!
//! # Abstract
//! Identifies VMs that are "zombies" (powered on, consuming RAM, but doing essentially zero work).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ZombieDetector, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 0.1, per_core: vec![0.1], load_avg: [0.0, 0.0, 0.0] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let score = snap.detect_zombie();
//! assert!(score.probability > 0.8);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The computed probability that a VM is a zombie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZombieScore {
    /// Probability from 0.0 to 1.0 (1.0 = definitely a zombie).
    pub probability: f64,
    /// Reasons contributing to the score.
    pub reasons: Vec<String>,
}

/// Trait to detect if a VM is a zombie.
pub trait ZombieDetector {
    /// Analyzes metrics to estimate zombie probability.
    fn detect_zombie(&self) -> ZombieScore;
}

impl ZombieDetector for MetricsSnapshot {
    fn detect_zombie(&self) -> ZombieScore {
        let mut probability: f64 = 0.0;
        let mut reasons = Vec::new();

        // Very low CPU
        if f64::from(self.cpu.total_pct) < 1.0 {
            probability += 0.5;
            reasons.push(format!(
                "CPU usage is extremely low ({:.2}%)",
                self.cpu.total_pct
            ));
        } else if f64::from(self.cpu.total_pct) < 5.0 {
            probability += 0.2;
            reasons.push(format!(
                "CPU usage is very low ({:.2}%)",
                self.cpu.total_pct
            ));
        }

        // Zero or negligible disk I/O
        let total_disk_io = self
            .disks
            .iter()
            .map(|d| d.read_bytes + d.write_bytes)
            .sum::<u64>();
        if total_disk_io < 1024 {
            probability += 0.25;
            reasons.push("Almost zero disk I/O detected".to_string());
        }

        // Zero or negligible network I/O
        let total_net_io = self
            .networks
            .iter()
            .map(|n| n.rx_bytes + n.tx_bytes)
            .sum::<u64>();
        if total_net_io < 1024 {
            probability += 0.25;
            reasons.push("Almost zero network I/O detected".to_string());
        }

        ZombieScore {
            probability: probability.clamp(0.0, 1.0),
            reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics};

    fn base_snap(cpu: f32, disk: u64, net: u64) -> MetricsSnapshot {
        let mut snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        if disk > 0 {
            snap.disks.push(DiskMetrics {
                name: "vda".to_string(),
                read_bytes: disk,
                write_bytes: 0,
                reads_total: 1,
                writes_total: 0,
            });
        }

        if net > 0 {
            snap.networks.push(NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: net,
                tx_bytes: 0,
                rx_packets: 1,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
            });
        }

        snap
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_definite_zombie() {
        let snap = base_snap(0.1, 0, 0);
        let score = snap.detect_zombie();
        assert_eq!(score.probability, 1.0);
        assert_eq!(score.reasons.len(), 3);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_not_a_zombie() {
        let snap = base_snap(20.0, 1024 * 1024, 1024 * 1024);
        let score = snap.detect_zombie();
        assert_eq!(score.probability, 0.0);
        assert!(score.reasons.is_empty());
    }

    #[test]
    fn test_partial_zombie() {
        let snap = base_snap(3.0, 0, 1024 * 1024);
        let score = snap.detect_zombie();
        assert!((score.probability - 0.45).abs() < f64::EPSILON);
        assert_eq!(score.reasons.len(), 2);
    }
}
