//! Billing and cost estimation module.
//!
//! # Abstract
//! Connects static resource allocations (`VmConfig`) with live utilization (`MetricsSnapshot` + `EfficiencyScorer`)
//! to calculate estimated hourly costs and identify wasted spend due to underutilization.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{BillingEstimator, PricingRates, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//! use std::path::PathBuf;
//!
//! let cfg = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096, // 4GB
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: hitz_api::GuestAgentMode::Disabled,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 0, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0
//!     },
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let rates = PricingRates { cpu_per_hour: 0.05, gb_ram_per_hour: 0.01 };
//! let estimate = cfg.estimate_billing(&rates, &snap);
//! assert!(estimate.total_hourly_cost > 0.0);
//! assert!(estimate.wasted_hourly_cost > 0.0);
//! ```

use crate::{EfficiencyScorer, MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Hourly pricing rates for VM resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRates {
    /// Cost per vCPU per hour.
    pub cpu_per_hour: f64,
    /// Cost per GB of RAM per hour.
    pub gb_ram_per_hour: f64,
}

/// Result of a billing estimation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingEstimate {
    /// Total estimated cost per hour based on allocated resources.
    pub total_hourly_cost: f64,
    /// Estimated wasted cost per hour based on inefficiency.
    pub wasted_hourly_cost: f64,
}

/// Trait to estimate billing costs.
pub trait BillingEstimator {
    /// Estimates hourly billing and waste.
    fn estimate_billing(&self, rates: &PricingRates, snap: &MetricsSnapshot) -> BillingEstimate;
}

impl BillingEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_billing(&self, rates: &PricingRates, snap: &MetricsSnapshot) -> BillingEstimate {
        let cpu_cost = f64::from(self.cpus) * rates.cpu_per_hour;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * rates.gb_ram_per_hour;

        let total_hourly_cost = cpu_cost + ram_cost;

        let eff = snap.calculate_efficiency();
        // Score is 0 to 100. Inefficiency percentage is (100 - score) / 100.
        let inefficiency_ratio = (100.0 - eff.score) / 100.0;

        let wasted_hourly_cost = total_hourly_cost * inefficiency_ratio;

        BillingEstimate {
            total_hourly_cost,
            wasted_hourly_cost,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};
    use std::path::PathBuf;

    fn dummy_config(cpus: u32, ram_mib: u32) -> VmConfig {
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
            guest_agent: crate::GuestAgentMode::Disabled,
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
    fn test_highly_efficient_billing() {
        let cfg = dummy_config(4, 4096);
        // High usage, efficiency score should be high (close to 100)
        let snap = dummy_snapshot(90.0, 4 * 1024 * 1024 * 1024, 3 * 1024 * 1024 * 1024);
        let rates = PricingRates {
            cpu_per_hour: 0.05,
            gb_ram_per_hour: 0.01,
        };

        let estimate = cfg.estimate_billing(&rates, &snap);

        #[allow(clippy::suboptimal_flops)]
        let expected_cost = (4.0 * 0.05) + (4.0 * 0.01); // 0.20 + 0.04 = 0.24
        assert!((estimate.total_hourly_cost - expected_cost).abs() < f64::EPSILON);
        assert!(estimate.wasted_hourly_cost < 0.05); // low waste
    }

    #[test]
    fn test_highly_inefficient_billing() {
        let cfg = dummy_config(8, 16384); // 8 CPUs, 16GB
        // Very low usage, efficiency score should be low
        let snap = dummy_snapshot(5.0, 16 * 1024 * 1024 * 1024, 512 * 1024 * 1024);
        let rates = PricingRates {
            cpu_per_hour: 0.05,
            gb_ram_per_hour: 0.01,
        };

        let estimate = cfg.estimate_billing(&rates, &snap);

        #[allow(clippy::suboptimal_flops)]
        let expected_cost = (8.0 * 0.05) + (16.0 * 0.01); // 0.40 + 0.16 = 0.56
        assert!((estimate.total_hourly_cost - expected_cost).abs() < f64::EPSILON);
        assert!(estimate.wasted_hourly_cost > 0.4); // high waste
    }
}
