//! Financial Operations (FinOps) cost estimation module.
//!
//! # Abstract
//! This module connects resource utilization (`MetricsSnapshot`) and configuration
//! (`VmConfig`) with a pricing model to estimate the real-time monetary cost of a VM,
//! dividing it into "active" (useful) and "wasted" (idle) spending.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, PricingModel, MetricsSnapshot, VmConfig, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! // Pricing: $0.05 per vCPU/hour, $0.01 per GB RAM/hour
//! let pricing = PricingModel::new(0.05, 0.01);
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0, // Only 10% utilized
//!         per_core: vec![10.0; 4],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 1024 * 1024 * 1024, // 25% utilized
//!         free_bytes: 3 * 1024 * 1024 * 1024,
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
//! let report = snap.analyze_cost(&config, &pricing);
//! assert!(report.wasted_hourly_cost > 0.0);
//! assert!(report.total_hourly_cost > report.active_hourly_cost);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable pricing model for VM resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per virtual CPU per hour.
    pub vcpu_hourly_rate: f64,
    /// Cost per GB (1024 MB) of RAM per hour.
    pub ram_gb_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel`.
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_gb_hourly_rate,
        }
    }
}

/// A breakdown of the estimated monetary cost for a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostReport {
    /// Total estimated cost per hour.
    pub total_hourly_cost: f64,
    /// The portion of the cost that is actively being used.
    pub active_hourly_cost: f64,
    /// The portion of the cost that is wasted (idle resources).
    pub wasted_hourly_cost: f64,
}

/// Trait to analyze financial operations and cost.
pub trait FinOpsAnalyzer {
    /// Analyzes the cost efficiency of the current metrics snapshot.
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> CostReport;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> CostReport {
        let vcpu_cost = f64::from(config.cpus) * pricing.vcpu_hourly_rate;
        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.ram_gb_hourly_rate;

        let total_cost = vcpu_cost + ram_cost;

        // Calculate utilization ratios
        let cpu_util = (f64::from(self.cpu.total_pct) / 100.0).clamp(0.0, 1.0);
        let mem_util = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64 / self.memory.total_bytes as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let active_cpu_cost = vcpu_cost * cpu_util;
        let active_ram_cost = ram_cost * mem_util;

        let active_cost = active_cpu_cost + active_ram_cost;
        let wasted_cost = total_cost - active_cost;

        CostReport {
            total_hourly_cost: total_cost,
            active_hourly_cost: active_cost,
            wasted_hourly_cost: wasted_cost,
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

    fn test_snapshot(cpu_pct: f32, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
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
    fn test_fully_utilized() {
        let config = test_config(4, 4096);
        let pricing = PricingModel::new(0.05, 0.01);
        let snap = test_snapshot(100.0, 4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024);

        let report = snap.analyze_cost(&config, &pricing);

        let expected_total = 4.0_f64.mul_add(0.05, 4.0 * 0.01);

        assert!((report.total_hourly_cost - expected_total).abs() < f64::EPSILON);
        assert!((report.active_hourly_cost - expected_total).abs() < f64::EPSILON);
        assert!((report.wasted_hourly_cost - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partially_utilized() {
        let config = test_config(4, 4096);
        let pricing = PricingModel::new(0.05, 0.01);
        let snap = test_snapshot(50.0, 4 * 1024 * 1024 * 1024, 2 * 1024 * 1024 * 1024);

        let report = snap.analyze_cost(&config, &pricing);

        let expected_total = 4.0_f64.mul_add(0.05, 4.0 * 0.01);
        let expected_active = expected_total / 2.0;

        assert!((report.total_hourly_cost - expected_total).abs() < f64::EPSILON);
        assert!((report.active_hourly_cost - expected_active).abs() < f64::EPSILON);
        assert!((report.wasted_hourly_cost - expected_active).abs() < f64::EPSILON);
    }

    #[test]
    fn test_zero_memory() {
        let config = test_config(4, 4096);
        let pricing = PricingModel::new(0.05, 0.01);
        let snap = test_snapshot(50.0, 0, 0);

        let report = snap.analyze_cost(&config, &pricing);

        let expected_total = 4.0_f64.mul_add(0.05, 4.0 * 0.01);
        let expected_active = (4.0_f64 * 0.05).mul_add(0.5, 0.0);

        assert!((report.total_hourly_cost - expected_total).abs() < f64::EPSILON);
        assert!((report.active_hourly_cost - expected_active).abs() < f64::EPSILON);
        assert!(report.wasted_hourly_cost > 0.0);
    }
}
