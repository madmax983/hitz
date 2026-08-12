//! FinOps Analyzer Module.
//!
//! # Abstract
//! This module connects the `VmConfig` (allocated resources) with the
//! `MetricsSnapshot` (utilized resources) to calculate estimated cloud costs
//! and identify financial waste.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, MetricsSnapshot, CloudPricing, FinOpsAnalyzer, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096, // 4 GB
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! // In a real scenario, this comes from the guest agent.
//! let snapshot = hitz_api::MetricsSnapshot {
//!     timestamp_ms: 0,
//!     cpu: hitz_api::CpuMetrics {
//!         total_pct: 25.0, // Using 25% of 4 cores
//!         per_core: vec![25.0, 25.0, 25.0, 25.0],
//!         load_avg: [1.0, 1.0, 1.0],
//!     },
//!     memory: hitz_api::MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 2 * 1024 * 1024 * 1024, // Using 2 GB
//!         free_bytes: 2 * 1024 * 1024 * 1024,
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
//! let analyzer = FinOpsAnalyzer::default();
//! let report = analyzer.analyze(&config, &snapshot);
//!
//! // We allocated 4 cores and 4 GB RAM, but only used 25% CPU and 50% RAM.
//! // The analyzer tells us how much money we are wasting!
//! assert!(report.wasted_monthly_cost > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Pricing rates for cloud resources (per hour).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPricing {
    /// Cost per vCPU per hour in USD.
    pub cpu_per_hour: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_per_gb_hour: f64,
}

impl Default for CloudPricing {
    fn default() -> Self {
        Self {
            // AWS Fargate us-east-1 approximate pricing
            cpu_per_hour: 0.04048,
            ram_per_gb_hour: 0.004_445,
        }
    }
}

/// The financial analysis of a VM's resource usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsReport {
    /// Estimated monthly cost if resources are fully utilized as allocated.
    pub allocated_monthly_cost: f64,
    /// Estimated monthly cost of actually utilized resources.
    pub utilized_monthly_cost: f64,
    /// The difference between allocated and utilized cost.
    pub wasted_monthly_cost: f64,
    /// A percentage score (0.0 to 100.0) indicating cost efficiency.
    pub efficiency_score: f64,
}

/// Analyzer to generate `FinOps` reports.
#[derive(Debug, Clone, Default)]
pub struct FinOpsAnalyzer {
    pricing: CloudPricing,
}

impl FinOpsAnalyzer {
    /// Create a new analyzer with custom pricing.
    #[must_use]
    pub const fn new(pricing: CloudPricing) -> Self {
        Self { pricing }
    }

    /// Calculate the `FinOps` report based on config and a snapshot.
    ///
    /// Assumes 730 hours in a month.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn analyze(&self, config: &VmConfig, snapshot: &MetricsSnapshot) -> FinOpsReport {
        let hours_per_month = 730.0;

        // Allocated costs
        let allocated_cpu_cost =
            f64::from(config.cpus) * self.pricing.cpu_per_hour * hours_per_month;
        let allocated_ram_gb = f64::from(config.ram_mib) / 1024.0;
        let allocated_ram_cost = allocated_ram_gb * self.pricing.ram_per_gb_hour * hours_per_month;
        let allocated_monthly_cost = allocated_cpu_cost + allocated_ram_cost;

        // Utilized costs (using snapshot as representative average)
        // Ensure total_pct is clamped between 0 and 100
        let cpu_util_pct = (f64::from(snapshot.cpu.total_pct)).clamp(0.0, 100.0);
        let utilized_cpu_cost = allocated_cpu_cost * (cpu_util_pct / 100.0);

        let used_ram_gb = (snapshot.memory.used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let utilized_ram_cost = used_ram_gb * self.pricing.ram_per_gb_hour * hours_per_month;

        let utilized_monthly_cost = utilized_cpu_cost + utilized_ram_cost;
        let wasted_monthly_cost = (allocated_monthly_cost - utilized_monthly_cost).max(0.0);

        let efficiency_score = if allocated_monthly_cost > 0.0 {
            (utilized_monthly_cost / allocated_monthly_cost) * 100.0
        } else {
            100.0
        };

        FinOpsReport {
            allocated_monthly_cost,
            utilized_monthly_cost,
            wasted_monthly_cost,
            efficiency_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn dummy_config() -> VmConfig {
        VmConfig {
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
        }
    }

    fn dummy_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 25.0, // 25% utilization
                per_core: vec![25.0, 25.0, 25.0, 25.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 4 * 1024 * 1024 * 1024,
                used_bytes: 2 * 1024 * 1024 * 1024, // 2 GB used (50%)
                free_bytes: 2 * 1024 * 1024 * 1024,
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
    fn test_finops_analysis() {
        let analyzer = FinOpsAnalyzer::default();
        let report = analyzer.analyze(&dummy_config(), &dummy_snapshot());

        // Allocated CPU: 4 * 0.04048 * 730 = 118.2016
        // Allocated RAM: 4 * 0.004_445 * 730 = 12.9794
        // Total Allocated = ~131.18

        assert!(report.allocated_monthly_cost > 131.0);
        assert!(report.allocated_monthly_cost < 132.0);

        // Utilized CPU: 25% of 118.2016 = 29.5504
        // Utilized RAM: 2GB (50%) of 12.9794 = 6.4897
        // Total Utilized = ~36.04

        assert!(report.utilized_monthly_cost > 36.0);
        assert!(report.utilized_monthly_cost < 37.0);

        // Wasted is the difference
        assert!(
            (report.allocated_monthly_cost
                - report.utilized_monthly_cost
                - report.wasted_monthly_cost)
                .abs()
                < f64::EPSILON
        );

        // Efficiency Score: ~36.04 / 131.18 * 100 = ~27.4%
        assert!(report.efficiency_score > 27.0);
        assert!(report.efficiency_score < 28.0);
    }
}
