//! Optimizer Module
//!
//! # Abstract
//! Combines real-time telemetry (Rightsizer) with infrastructure as code (Terraform)
//! to export a perfectly sized Terraform HCL configuration based on actual workload usage.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{OptimizeTerraform, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 512,
//!     cpus: 2,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![95.0, 95.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 512 * 1024 * 1024, used_bytes: 500 * 1024 * 1024, free_bytes: 0, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let hcl = snap.to_optimized_terraform(&config, "auto_vm");
//! assert!(hcl.contains("# Optimized by hitz-api"));
//! assert!(hcl.contains("cpus = 3"));
//! ```

use crate::{MetricsSnapshot, ResizeRecommendation, RightSizer, ToTerraform, VmConfig};
use std::fmt::Write as _;

/// Trait to export a metrics-optimized Terraform HCL representation.
pub trait OptimizeTerraform {
    /// Returns the optimized Terraform HCL as a String.
    fn to_optimized_terraform(&self, config: &VmConfig, resource_name: &str) -> String;
}

impl OptimizeTerraform for MetricsSnapshot {
    fn to_optimized_terraform(&self, config: &VmConfig, resource_name: &str) -> String {
        let recommendations = self.recommend_sizing(config);

        let mut optimized_config = config.clone();
        let mut reasons = Vec::new();

        for rec in recommendations {
            match rec {
                ResizeRecommendation::ScaleUpCpu {
                    suggested, reason, ..
                }
                | ResizeRecommendation::ScaleDownCpu {
                    suggested, reason, ..
                } => {
                    optimized_config.cpus = suggested;
                    reasons.push(reason);
                }
                ResizeRecommendation::ScaleUpRam {
                    suggested_mib,
                    reason,
                    ..
                }
                | ResizeRecommendation::ScaleDownRam {
                    suggested_mib,
                    reason,
                    ..
                } => {
                    optimized_config.ram_mib = suggested_mib;
                    reasons.push(reason);
                }
            }
        }

        let mut hcl = String::from("# Optimized by hitz-api\n");
        for reason in reasons {
            let _ = writeln!(hcl, "# Reason: {reason}");
        }

        hcl.push_str(&optimized_config.to_terraform(resource_name));
        hcl
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
                total_bytes: 1024 * 1024 * 1024, // Assuming max for test consistency
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
    fn test_optimize_scale_up() {
        let config = test_config(2, 512);
        // We need to trigger the scale up conditions in Rightsizer
        // cpu > 90.0, mem_pct > 90.0
        // Memory test logic in Rightsizer compares used_bytes with config ram bytes.
        // For 512 MiB config, 90% is 460 MiB.
        let snap = test_snapshot(95.0, 500);

        let optimized_hcl = snap.to_optimized_terraform(&config, "optimized_vm");

        assert!(optimized_hcl.contains("# Optimized by hitz-api"));
        assert!(optimized_hcl.contains("cpus = 3")); // Scaled up
        assert!(optimized_hcl.contains("ram_mib = 1024")); // Scaled up
        assert!(optimized_hcl.contains("# Reason: CPU utilization is critically high at 95.0%"));
    }

    #[test]
    fn test_optimize_scale_down() {
        let config = test_config(4, 2048);
        // Trigger scale down: cpu < 10.0, mem_pct < 20.0
        // For 2048 MiB, 20% is 409 MiB.
        let snap = test_snapshot(5.0, 200);

        let optimized_hcl = snap.to_optimized_terraform(&config, "optimized_vm");

        assert!(optimized_hcl.contains("# Optimized by hitz-api"));
        assert!(optimized_hcl.contains("cpus = 3")); // Scaled down
        assert!(optimized_hcl.contains("ram_mib = 1024")); // Scaled down
    }

    #[test]
    fn test_optimize_healthy() {
        let config = test_config(4, 2048);
        let snap = test_snapshot(50.0, 1024); // Healthy load

        let optimized_hcl = snap.to_optimized_terraform(&config, "optimized_vm");

        assert!(optimized_hcl.contains("# Optimized by hitz-api"));
        assert!(optimized_hcl.contains("cpus = 4")); // Unchanged
        assert!(optimized_hcl.contains("ram_mib = 2048")); // Unchanged
    }
}
