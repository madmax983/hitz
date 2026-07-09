//! VM Rightsizing Recommendations.
//!
//! # Abstract
//! This module analyzes real-time telemetry (`MetricsSnapshot`) against the
//! current configuration (`VmConfig`) to provide actionable recommendations
//! for scaling resources up or down, optimizing for both performance and cost.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{RightSizer, ResizeRecommendation, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // Assuming we have a heavily underutilized VM
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 5.0,
//!         per_core: vec![5.0; 4],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 900 * 1024 * 1024,
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
//! let recommendations = snap.recommend_sizing(&config);
//! assert!(recommendations.len() > 0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Specific actionable recommendation for resizing a VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResizeRecommendation {
    /// Suggests increasing the CPU count.
    ScaleUpCpu {
        /// The current CPU count.
        current: u32,
        /// The suggested new CPU count.
        suggested: u32,
        /// The reason for this recommendation.
        reason: String,
    },
    /// Suggests decreasing the CPU count.
    ScaleDownCpu {
        /// The current CPU count.
        current: u32,
        /// The suggested new CPU count.
        suggested: u32,
        /// The reason for this recommendation.
        reason: String,
    },
    /// Suggests increasing the RAM allocation.
    ScaleUpRam {
        /// The current RAM allocation in MiB.
        current_mib: u32,
        /// The suggested new RAM allocation in MiB.
        suggested_mib: u32,
        /// The reason for this recommendation.
        reason: String,
    },
    /// Suggests decreasing the RAM allocation.
    ScaleDownRam {
        /// The current RAM allocation in MiB.
        current_mib: u32,
        /// The suggested new RAM allocation in MiB.
        suggested_mib: u32,
        /// The reason for this recommendation.
        reason: String,
    },
}

/// Trait for objects that can evaluate metrics and configuration to recommend resizing.
pub trait RightSizer {
    /// Analyzes current metrics against the VM's configuration and returns recommendations.
    ///
    /// # Abstract
    /// Evaluates CPU and Memory utilization against the provisioned capacity in the configuration
    /// and generates a list of scaling recommendations.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{RightSizer, ResizeRecommendation, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
    /// use std::path::PathBuf;
    ///
    /// let config = VmConfig {
    ///     kernel_path: PathBuf::from("vmlinux"),
    ///     initramfs_path: None,
    ///     disk_path: None,
    ///     ram_mib: 1024,
    ///     cpus: 4,
    ///     cmdline: None,
    ///     net: None,
    ///     ports: vec![],
    ///     guest_cid: 3,
    ///     guest_agent: GuestAgentMode::Auto,
    /// };
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics { total_pct: 5.0, per_core: vec![5.0; 4], load_avg: [0.1, 0.1, 0.1] },
    ///     memory: MemoryMetrics { total_bytes: 1024 * 1024 * 1024, used_bytes: 100 * 1024 * 1024, free_bytes: 900 * 1024 * 1024, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
    ///     disks: vec![],
    ///     networks: vec![],
    ///     processes: vec![],
    /// };
    ///
    /// let recommendations = snap.recommend_sizing(&config);
    /// assert!(recommendations.len() > 0);
    /// ```
    fn recommend_sizing(&self, config: &VmConfig) -> Vec<ResizeRecommendation>;
}

impl RightSizer for MetricsSnapshot {
    fn recommend_sizing(&self, config: &VmConfig) -> Vec<ResizeRecommendation> {
        // ⚡ Bolt Optimization: Pre-allocate capacity for exactly 2 recommendations
        // (CPU and RAM) to avoid dynamic heap reallocations during assessment.
        let mut recs = Vec::with_capacity(2);

        // CPU Analysis
        if self.cpu.total_pct > 90.0 {
            recs.push(ResizeRecommendation::ScaleUpCpu {
                current: config.cpus,
                suggested: config.cpus.saturating_add(1),
                reason: format!(
                    "CPU utilization is critically high at {:.1}%",
                    self.cpu.total_pct
                ),
            });
        } else if self.cpu.total_pct < 10.0 && config.cpus > 1 {
            recs.push(ResizeRecommendation::ScaleDownCpu {
                current: config.cpus,
                suggested: config.cpus.saturating_sub(1).max(1),
                reason: format!("CPU utilization is very low at {:.1}%", self.cpu.total_pct),
            });
        }

        // Memory Analysis
        let ram_bytes = u64::from(config.ram_mib) * 1024 * 1024;
        let used_bytes = self.memory.used_bytes;

        if ram_bytes > 0 {
            #[allow(clippy::cast_precision_loss)]
            let mem_pct = (used_bytes as f64 / ram_bytes as f64) * 100.0;

            if mem_pct > 90.0 {
                let suggested_mib = config.ram_mib.saturating_mul(2);
                recs.push(ResizeRecommendation::ScaleUpRam {
                    current_mib: config.ram_mib,
                    suggested_mib,
                    reason: format!("Memory utilization is critically high at {mem_pct:.1}%"),
                });
            } else if mem_pct < 20.0 && config.ram_mib > 128 {
                let suggested_mib = config.ram_mib.saturating_div(2).max(128);
                recs.push(ResizeRecommendation::ScaleDownRam {
                    current_mib: config.ram_mib,
                    suggested_mib,
                    reason: format!("Memory utilization is very low at {mem_pct:.1}%"),
                });
            }
        }

        recs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn test_config(cpus: u32, ram_mib: u32) -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib,
            cpus,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    fn test_snapshot(cpu_pct: f32, mem_used_mib: u32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: u64::from(mem_used_mib) * 1024 * 1024,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        }
    }

    #[test]
    fn test_no_recommendation_for_healthy_vm() {
        let config = test_config(2, 512);
        let snap = test_snapshot(50.0, 256); // 50% CPU, 50% RAM
        let recs = snap.recommend_sizing(&config);
        assert!(
            recs.is_empty(),
            "Healthy VM should not have sizing recommendations"
        );
    }

    #[test]
    fn test_scale_up_cpu_and_ram() {
        let config = test_config(2, 512);
        let snap = test_snapshot(95.0, 500); // 95% CPU, ~97% RAM
        let recs = snap.recommend_sizing(&config);

        assert_eq!(recs.len(), 2);
        assert!(matches!(
            recs[0],
            ResizeRecommendation::ScaleUpCpu { suggested: 3, .. }
        ));
        assert!(matches!(
            recs[1],
            ResizeRecommendation::ScaleUpRam {
                suggested_mib: 1024,
                ..
            }
        ));
    }

    #[test]
    fn test_scale_down_cpu_and_ram() {
        let config = test_config(4, 2048);
        let snap = test_snapshot(5.0, 200); // 5% CPU, ~9% RAM
        let recs = snap.recommend_sizing(&config);

        assert_eq!(recs.len(), 2);
        assert!(matches!(
            recs[0],
            ResizeRecommendation::ScaleDownCpu { suggested: 3, .. }
        ));
        assert!(matches!(
            recs[1],
            ResizeRecommendation::ScaleDownRam {
                suggested_mib: 1024,
                ..
            }
        ));
    }

    #[test]
    fn test_scale_down_limits() {
        let config = test_config(1, 128); // Already at minimums
        let snap = test_snapshot(2.0, 10); // Very low usage
        let recs = snap.recommend_sizing(&config);

        // Should not recommend going below 1 CPU or 128 MiB RAM
        assert!(
            recs.is_empty(),
            "Should not recommend scaling below minimum limits"
        );
    }
}
