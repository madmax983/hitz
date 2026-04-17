//! Recommender module for right-sizing VMs based on metrics.
//!
//! # Abstract
//! This module introduces the ability to automatically suggest optimizations
//! for micro-VMs. By evaluating telemetry patterns, it recommends scaling up
//! CPU or memory when a VM is constrained, or scaling down to reclaim resources
//! when a VM is idle.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{RightSizer, MetricsSnapshot, VmConfig, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // We have a VM configuration and its current metrics
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 512,
//!     cpus: 1,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let metrics = MetricsSnapshot {
//!     timestamp_ms: 1_700_000_000_000,
//!     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![], load_avg: [0.0, 0.0, 0.0] },
//!     memory: MemoryMetrics { total_bytes: 536_870_912, used_bytes: 134_217_728, free_bytes: 402_653_184, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // The VM is CPU-starved! Let's get a recommendation to scale up
//! let recommended_config = metrics.recommend(&config);
//! assert_eq!(recommended_config.cpus, 2);
//! ```

use crate::{MetricsSnapshot, VmConfig};

/// Trait for recommending a right-sized configuration based on current state.
///
/// Implementations analyze the provided metrics and suggest alterations to
/// the `VmConfig`, ensuring limits (such as a minimum of 256 MiB RAM or 1 CPU)
/// are respected.
pub trait RightSizer {
    /// Evaluates the current state and recommends an updated VM configuration.
    ///
    /// If no changes are recommended, this should return the original configuration.
    #[must_use]
    fn recommend(&self, current_config: &VmConfig) -> VmConfig;
}

impl RightSizer for MetricsSnapshot {
    fn recommend(&self, current_config: &VmConfig) -> VmConfig {
        let mut new_config = current_config.clone();

        // Evaluate CPU right-sizing
        if self.cpu.total_pct > 80.0 {
            // Starved: Add a vCPU
            new_config.cpus += 1;
        } else if self.cpu.total_pct < 20.0 {
            // Idle: Reclaim a vCPU, ensuring at least 1 remains
            new_config.cpus = new_config.cpus.saturating_sub(1).max(1);
        }

        // Evaluate Memory right-sizing
        #[allow(clippy::cast_precision_loss)]
        let used_pct = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        if used_pct > 80.0 {
            // Starved: Add 128 MiB RAM
            new_config.ram_mib += 128;
        } else if used_pct < 20.0 {
            // Idle: Reclaim 128 MiB RAM, ensuring at least 256 MiB remains
            new_config.ram_mib = new_config.ram_mib.saturating_sub(128).max(256);
        }

        new_config
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DEFAULT_GUEST_CID, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn base_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 512,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    fn mock_metrics(cpu_pct: f32, ram_used_mib: u64, ram_total_mib: u64) -> MetricsSnapshot {
        let mb = 1024 * 1024;
        MetricsSnapshot {
            timestamp_ms: 1_700_000_000_000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: ram_total_mib * mb,
                used_bytes: ram_used_mib * mb,
                free_bytes: (ram_total_mib - ram_used_mib) * mb,
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
    fn test_recommend_scale_up() {
        let cfg = base_config();
        // 95% CPU, 90% RAM (460/512 MiB)
        let metrics = mock_metrics(95.0, 460, 512);

        let new_cfg = metrics.recommend(&cfg);

        assert_eq!(new_cfg.cpus, 3, "Expected CPU count to scale up to 3");
        assert_eq!(new_cfg.ram_mib, 640, "Expected RAM to scale up to 640 MiB");
    }

    #[test]
    fn test_recommend_scale_down() {
        let cfg = base_config();
        // 5% CPU, 10% RAM (50/512 MiB)
        let metrics = mock_metrics(5.0, 50, 512);

        let new_cfg = metrics.recommend(&cfg);

        assert_eq!(new_cfg.cpus, 1, "Expected CPU count to scale down to 1");
        assert_eq!(
            new_cfg.ram_mib, 384,
            "Expected RAM to scale down to 384 MiB"
        );
    }

    #[test]
    fn test_recommend_optimal() {
        let cfg = base_config();
        // 50% CPU, 50% RAM (256/512 MiB)
        let metrics = mock_metrics(50.0, 256, 512);

        let new_cfg = metrics.recommend(&cfg);

        assert_eq!(new_cfg.cpus, 2, "Expected optimal CPU to remain unchanged");
        assert_eq!(
            new_cfg.ram_mib, 512,
            "Expected optimal RAM to remain unchanged"
        );
    }

    #[test]
    fn test_recommend_edge_case_limits() {
        let mut cfg = base_config();
        cfg.cpus = 1;
        cfg.ram_mib = 256;

        // Very idle
        let metrics = mock_metrics(5.0, 10, 256);
        let new_cfg = metrics.recommend(&cfg);

        assert_eq!(new_cfg.cpus, 1, "Expected CPU to bottom out at 1");
        assert_eq!(
            new_cfg.ram_mib, 256,
            "Expected RAM to bottom out at 256 MiB"
        );
    }
}
