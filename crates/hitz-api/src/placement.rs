//! VM Placement Scheduler Module.
//!
//! # Abstract
//! This module analyzes candidate host metrics to determine the optimal placement
//! for a new VM based on its configuration requirements.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{PlacementEngine, HostCandidate, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 2,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let host_metrics = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0, 10.0, 10.0, 10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 16 * 1024 * 1024 * 1024,
//!         used_bytes: 4 * 1024 * 1024 * 1024,
//!         free_bytes: 12 * 1024 * 1024 * 1024,
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
//! let candidates = vec![HostCandidate { id: "host1".to_string(), metrics: host_metrics }];
//! let best_host = config.find_best_placement(&candidates).unwrap();
//! assert_eq!(best_host.host_id, "host1");
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Represents a potential host for VM placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostCandidate {
    /// The unique identifier of the host.
    pub id: String,
    /// The current telemetry of the host.
    pub metrics: MetricsSnapshot,
}

/// Represents the scoring result of a host candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementScore {
    /// The host identifier.
    pub host_id: String,
    /// The suitability score (higher is better).
    pub score: f64,
}

/// Trait for finding the best placement for a VM.
pub trait PlacementEngine {
    /// Evaluates a list of host candidates and returns the best placement.
    fn find_best_placement(&self, candidates: &[HostCandidate]) -> Option<PlacementScore>;
}

impl PlacementEngine for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn find_best_placement(&self, candidates: &[HostCandidate]) -> Option<PlacementScore> {
        let required_ram_bytes = u64::from(self.ram_mib) * 1024 * 1024;
        let mut best_score = -1.0;
        let mut best_candidate = None;

        for candidate in candidates {
            // Filter out hosts without enough free memory
            if candidate.metrics.memory.free_bytes < required_ram_bytes {
                continue;
            }

            // Simple scoring: prefer hosts with more free memory and lower CPU usage
            let mem_score = (candidate.metrics.memory.free_bytes - required_ram_bytes) as f64
                / (1024.0 * 1024.0 * 1024.0);
            let cpu_score = 100.0 - f64::from(candidate.metrics.cpu.total_pct);

            // Give CPU more weight in this simple model
            let score = mem_score + (cpu_score * 2.0);

            if score > best_score {
                best_score = score;
                best_candidate = Some(PlacementScore {
                    host_id: candidate.id.clone(),
                    score,
                });
            }
        }

        best_candidate
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn test_config(ram_mib: u32) -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    fn test_host(id: &str, cpu_pct: f32, free_ram_gb: u64) -> HostCandidate {
        HostCandidate {
            id: id.to_string(),
            metrics: MetricsSnapshot {
                timestamp_ms: 1000,
                cpu: CpuMetrics {
                    total_pct: cpu_pct,
                    per_core: vec![cpu_pct],
                    load_avg: [0.1, 0.1, 0.1],
                },
                memory: MemoryMetrics {
                    total_bytes: 32 * 1024 * 1024 * 1024,
                    used_bytes: (32 - free_ram_gb) * 1024 * 1024 * 1024,
                    free_bytes: free_ram_gb * 1024 * 1024 * 1024,
                    buffers_bytes: 0,
                    cached_bytes: 0,
                    swap_total: 0,
                    swap_used: 0,
                },
                disks: vec![],
                networks: vec![],
                processes: vec![],
            },
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_find_best_placement_success() {
        let config = test_config(2048); // 2GB
        let candidates = vec![
            test_host("host1", 80.0, 4),  // High CPU, enough RAM
            test_host("host2", 20.0, 16), // Low CPU, lots of RAM
            test_host("host3", 10.0, 1),  // Low CPU, not enough RAM
        ];

        let result = config.find_best_placement(&candidates);
        assert!(result.is_some());
        let score = result.unwrap();
        assert_eq!(score.host_id, "host2");
    }

    #[test]
    fn test_find_best_placement_none() {
        let config = test_config(8192); // 8GB
        let candidates = vec![
            test_host("host1", 10.0, 4), // Not enough RAM
            test_host("host2", 20.0, 2), // Not enough RAM
        ];

        let result = config.find_best_placement(&candidates);
        assert!(result.is_none());
    }
}
