//! FinOps scoring module.
//!
//! # Abstract
//! Calculates financial waste based on resource underutilization and pricing models.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, FinOpsWaste, PricingModel, MetricsSnapshot, CpuMetrics, MemoryMetrics, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 0,
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
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096,
//!     cpus: 1,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let pricing = PricingModel::new(0.05, 0.01); // $0.05 per CPU/hr, $0.01 per GB RAM/hr
//! let waste = snap.calculate_waste(&config, &pricing);
//! assert!(waste.wasted_cost_per_hour > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable pricing model for resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour in USD.
    pub cost_per_vcpu_hour: f64,
    /// Cost per GB of RAM per hour in USD.
    pub cost_per_gb_ram_hour: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel`.
    #[must_use]
    pub const fn new(cost_per_vcpu_hour: f64, cost_per_gb_ram_hour: f64) -> Self {
        Self {
            cost_per_vcpu_hour,
            cost_per_gb_ram_hour,
        }
    }
}

/// The computed financial waste based on resource underutilization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsWaste {
    /// The estimated wasted cost per hour in USD.
    pub wasted_cost_per_hour: f64,
    /// The detailed reasoning for the estimated waste.
    pub reasons: Vec<String>,
}

/// Trait to calculate financial waste.
pub trait FinOpsAnalyzer {
    /// Calculates financial waste from metrics and configuration.
    fn calculate_waste(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsWaste;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn calculate_waste(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsWaste {
        let mut wasted_cost = 0.0;
        let mut reasons = Vec::with_capacity(2);

        // CPU evaluation
        let cpu_usage = f64::from(self.cpu.total_pct);
        if cpu_usage < 40.0 {
            // Assume < 40% usage is wasting the unused portion.
            let unused_pct = (40.0 - cpu_usage) / 100.0;
            let cpu_waste = unused_pct * f64::from(config.cpus) * pricing.cost_per_vcpu_hour;
            wasted_cost += cpu_waste;
            reasons.push(format!(
                "CPU underutilized ({cpu_usage:.1}%): Wasting ${cpu_waste:.4}/hr."
            ));
        }

        // Memory evaluation
        let ram_bytes = u64::from(config.ram_mib) * 1024 * 1024;
        let used_bytes = self.memory.used_bytes;

        if ram_bytes > 0 {
            let mem_usage_pct = (used_bytes as f64 / ram_bytes as f64) * 100.0;
            if mem_usage_pct < 50.0 {
                // Assume < 50% usage is wasting the unused portion.
                let unused_pct = (50.0 - mem_usage_pct) / 100.0;
                let ram_gb = f64::from(config.ram_mib) / 1024.0;
                let ram_waste = unused_pct * ram_gb * pricing.cost_per_gb_ram_hour;
                wasted_cost += ram_waste;
                reasons.push(format!(
                    "Memory underutilized ({mem_usage_pct:.1}%): Wasting ${ram_waste:.4}/hr."
                ));
            }
        }

        FinOpsWaste {
            wasted_cost_per_hour: wasted_cost,
            reasons,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
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

    fn dummy_snapshot(cpu: f32, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total,
                used_bytes: mem_used,
                free_bytes: mem_total.saturating_sub(mem_used),
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
    fn test_highly_efficient_no_waste() {
        let config = test_config(4, 4096);
        let snap = dummy_snapshot(90.0, 4 * 1024 * 1024 * 1024, 3 * 1024 * 1024 * 1024);
        let pricing = PricingModel::new(0.05, 0.01);

        let waste = snap.calculate_waste(&config, &pricing);
        assert!((waste.wasted_cost_per_hour - 0.0).abs() < f64::EPSILON);
        assert!(waste.reasons.is_empty());
    }

    #[test]
    fn test_underutilized_waste() {
        let config = test_config(4, 4096);
        let snap = dummy_snapshot(10.0, 4 * 1024 * 1024 * 1024, 1024 * 1024 * 1024); // 10% CPU, 25% RAM
        let pricing = PricingModel::new(0.05, 0.01);

        let waste = snap.calculate_waste(&config, &pricing);
        assert!(waste.wasted_cost_per_hour > 0.0);
        assert_eq!(waste.reasons.len(), 2);
    }
}
