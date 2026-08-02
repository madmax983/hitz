//! Financial Operations (FinOps) cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the absolute resource
//! utilization of a system (`MetricsSnapshot`) with configurable pricing rates
//! to estimate the real-time cost of the VM in a cloud environment.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingRates, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 50.0,
//!         per_core: vec![50.0, 50.0],
//!         load_avg: [1.0, 1.0, 1.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 2 * 1024 * 1024 * 1024,
//!         used_bytes: 1024 * 1024 * 1024,
//!         free_bytes: 1024 * 1024 * 1024,
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
//! let rates = PricingRates {
//!     cpu_per_core_hour: 0.05,
//!     ram_per_gb_hour: 0.01,
//! };
//!
//! let cost = snap.estimate_cost_per_hour(&rates, 2);
//! assert!(cost > 0.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing rates for cloud resources.
///
/// # Abstract
/// This struct holds the assumptions needed to translate resource consumption
/// into financial costs per hour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRates {
    /// Cost per CPU core per hour.
    pub cpu_per_core_hour: f64,
    /// Cost per GB of RAM per hour.
    pub ram_per_gb_hour: f64,
}

/// Trait for objects that can estimate their financial cost.
pub trait CostEstimator {
    /// Estimates the current rate of financial cost per hour based on utilization.
    fn estimate_cost_per_hour(&self, rates: &PricingRates, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost_per_hour(&self, rates: &PricingRates, vcpus: u32) -> f64 {
        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let vcpus_f64 = f64::from(vcpus);
        let cpu_cost = vcpus_f64 * cpu_utilization * rates.cpu_per_core_hour;

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = ram_gb * rates.ram_per_gb_hour;

        cpu_cost + ram_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_cost_estimator() {
        let rates = PricingRates {
            cpu_per_core_hour: 0.05,
            ram_per_gb_hour: 0.01,
        };
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 100.0,
                per_core: vec![100.0, 100.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 2 * 1024 * 1024 * 1024,
                used_bytes: 1024 * 1024 * 1024,
                free_bytes: 1024 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let cost = snap.estimate_cost_per_hour(&rates, 2);
        assert!((cost - 0.12).abs() < f64::EPSILON, "Expected 0.12, got {cost}");
    }
}
