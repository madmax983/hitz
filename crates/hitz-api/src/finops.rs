//! FinOps module for tracking financial cost and waste.
//!
//! # Abstract
//! Connects VM configuration and real-time metrics to a pricing model to estimate
//! hourly costs and financially quantify resource waste based on efficiency scores.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, PricingModel, VmConfig, MetricsSnapshot, GuestAgentMode, CpuMetrics, MemoryMetrics};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0, 10.0, 10.0, 10.0],
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
//! let pricing = PricingModel {
//!     vcpu_hourly_rate: 0.05,
//!     ram_gb_hourly_rate: 0.01,
//! };
//!
//! let report = snap.analyze_cost(&config, &pricing);
//! assert!(report.total_hourly_cost > 0.0);
//! assert!(report.wasted_hourly_cost > 0.0);
//! ```

use crate::{EfficiencyScorer, MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable pricing model for VM resources.
///
/// # Abstract
/// Represents the hourly cost of compute and memory resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour.
    pub vcpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour.
    pub ram_gb_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new pricing model.
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_gb_hourly_rate,
        }
    }
}

/// The computed financial report for a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsReport {
    /// Total cost per hour based on allocated resources.
    pub total_hourly_cost: f64,
    /// Estimated wasted cost per hour based on underutilization.
    pub wasted_hourly_cost: f64,
    /// Efficiency score used to calculate the waste (0.0 to 100.0).
    pub efficiency_score: f64,
}

/// Trait for objects that can analyze financial costs.
pub trait FinOpsAnalyzer {
    /// Calculates the financial cost and waste of the system.
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsReport;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsReport {
        let vcpus_f64 = f64::from(config.cpus);
        let ram_gb = f64::from(config.ram_mib) / 1024.0;

        let total_hourly_cost = vcpus_f64.mul_add(
            pricing.vcpu_hourly_rate,
            ram_gb * pricing.ram_gb_hourly_rate,
        );

        let efficiency = self.calculate_efficiency();
        let waste_pct = (100.0 - efficiency.score) / 100.0;
        let wasted_hourly_cost = total_hourly_cost * waste_pct;

        FinOpsReport {
            total_hourly_cost,
            wasted_hourly_cost,
            efficiency_score: efficiency.score,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

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
    fn test_finops_high_efficiency() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 4096, // 4 GB
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        // High CPU and memory usage -> 100% efficient
        let snap = dummy_snapshot(90.0, 4 * 1024 * 1024 * 1024, 3 * 1024 * 1024 * 1024);

        let pricing = PricingModel::new(0.05, 0.01);
        let report = snap.analyze_cost(&config, &pricing);

        // Cost = 4 * 0.05 + 4 * 0.01 = 0.20 + 0.04 = 0.24
        assert!((report.total_hourly_cost - 0.24).abs() < f64::EPSILON);
        assert!((report.wasted_hourly_cost - 0.0).abs() < f64::EPSILON);
        assert!((report.efficiency_score - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_finops_low_efficiency() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 4096, // 4 GB
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        // Low CPU and memory usage -> not 100% efficient
        let snap = dummy_snapshot(5.0, 4 * 1024 * 1024 * 1024, 100 * 1024 * 1024);

        let pricing = PricingModel::new(0.05, 0.01);
        let report = snap.analyze_cost(&config, &pricing);

        assert!((report.total_hourly_cost - 0.24).abs() < f64::EPSILON);
        assert!(report.wasted_hourly_cost > 0.0);
        assert!(report.efficiency_score < 100.0);
    }
}
