//! Auto-tuning module for Virtual Machines.
//!
//! # Abstract
//! Uses real-time metrics (`MetricsSnapshot`) to analyze configuration efficiency
//! (via `RightSizer`) and outputs an updated infrastructure configuration
//! (via `ToTerraform`).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{AutoTuner, MetricsSnapshot, VmConfig, GuestAgentMode, CpuMetrics, MemoryMetrics};
//! use std::path::PathBuf;
//!
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
//!         total_pct: 95.0,
//!         per_core: vec![95.0; 4],
//!         load_avg: [1.0, 1.0, 1.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 1000 * 1024 * 1024,
//!         free_bytes: 24 * 1024 * 1024,
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
//! let updated_hcl = config.auto_tune("tuned_vm", &snap);
//! assert!(updated_hcl.contains("cpus = 5"));
//! assert!(updated_hcl.contains("ram_mib = 2048"));
//! ```

use crate::{MetricsSnapshot, ResizeRecommendation, RightSizer, ToTerraform, VmConfig};

/// Trait to automatically tune and export VM configurations based on telemetry.
pub trait AutoTuner {
    /// Generates a new Terraform configuration string reflecting optimized
    /// resource allocations based on the provided metrics snapshot.
    fn auto_tune(&self, resource_name: &str, snapshot: &MetricsSnapshot) -> String;
}

impl AutoTuner for VmConfig {
    fn auto_tune(&self, resource_name: &str, snapshot: &MetricsSnapshot) -> String {
        let mut optimized_config = self.clone();

        let recommendations = snapshot.recommend_sizing(self);
        for rec in recommendations {
            match rec {
                ResizeRecommendation::ScaleUpCpu { suggested, .. }
                | ResizeRecommendation::ScaleDownCpu { suggested, .. } => {
                    optimized_config.cpus = suggested;
                }
                ResizeRecommendation::ScaleUpRam { suggested_mib, .. }
                | ResizeRecommendation::ScaleDownRam { suggested_mib, .. } => {
                    optimized_config.ram_mib = suggested_mib;
                }
            }
        }

        optimized_config.to_terraform(resource_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn base_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    #[test]
    fn test_auto_tune_no_changes() {
        let config = base_config();
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0; 4],
                load_avg: [0.5, 0.5, 0.5],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
                free_bytes: 512 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let hcl = config.auto_tune("stable_vm", &snap);
        assert!(hcl.contains("cpus = 4"));
        assert!(hcl.contains("ram_mib = 1024"));
    }

    #[test]
    fn test_auto_tune_scale_up() {
        let config = base_config();
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 95.0,
                per_core: vec![95.0; 4],
                load_avg: [1.5, 1.5, 1.5],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 1000 * 1024 * 1024,
                free_bytes: 24 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let hcl = config.auto_tune("stressed_vm", &snap);
        assert!(hcl.contains("cpus = 5"));
        assert!(hcl.contains("ram_mib = 2048"));
    }
}
