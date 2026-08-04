//! `FinOps` cost estimation module for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the resource
//! configuration and utilization of a system with a configurable pricing model
//! to estimate the running cost of the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 100.0,
//!         per_core: vec![100.0, 100.0],
//!         load_avg: [1.0, 1.0, 1.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024, // 1 GB
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 512 * 1024 * 1024,
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
//! let factors = PricingFactors::new(0.02, 0.005);
//! let cost = snap.estimate_monthly_cost(&factors, 2);
//! assert!(cost > 0.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for cost estimation.
///
/// # Abstract
/// This struct holds the assumptions needed to translate compute and memory
/// allocations into monetary costs in USD.
///
/// # Details
/// It includes hourly rates for vCPUs and RAM, as well as an optional premium
/// for active resource utilization, modeling burstable instances or network/IO overhead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per vCPU hour in USD.
    pub vcpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_gb_hourly_rate: f64,
    /// Premium multiplier applied to utilized CPU cycles (default 0.0 means no premium).
    pub utilization_premium_rate: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` profile with standard rates.
    ///
    /// # Abstract
    /// A quick factory method to instantiate pricing factors.
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_gb_hourly_rate,
            utilization_premium_rate: 0.0,
        }
    }
}

/// Trait for objects that can estimate their running cost.
///
/// # Abstract
/// This trait defines the capability to estimate the real-time monetary cost
/// of a micro-VM based on its current resource allocation and utilization.
pub trait CostEstimator {
    /// Estimates the current monthly cost in USD (assuming 730 hours per month).
    ///
    /// # Abstract
    /// Applies a pricing model to a utilization snapshot to estimate the projected monthly cost.
    fn estimate_monthly_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_monthly_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64 {
        let hours_per_month = 730.0;
        let vcpus_f64 = f64::from(vcpus);
        let base_cpu_cost = vcpus_f64 * factors.vcpu_hourly_rate * hours_per_month;

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let base_ram_cost = ram_gb * factors.ram_gb_hourly_rate * hours_per_month;

        let base_cost = base_cpu_cost + base_ram_cost;

        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let active_premium = base_cost * factors.utilization_premium_rate * cpu_utilization;

        base_cost + active_premium
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(cpu_pct: f32, ram_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_bytes,
                used_bytes: ram_bytes / 2,
                free_bytes: ram_bytes / 2,
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
    fn test_cost_estimator_base() {
        let factors = PricingFactors::new(0.02, 0.005);
        let snap = base_snapshot(0.0, 2 * 1024 * 1024 * 1024); // 2GB RAM

        // 4 vcpus * 0.02 * 730 = 58.4
        // 2 GB * 0.005 * 730 = 7.3
        // Total = 65.7
        let cost = snap.estimate_monthly_cost(&factors, 4);
        assert!((cost - 65.7).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::suboptimal_flops)]
    fn test_cost_estimator_with_premium() {
        let mut factors = PricingFactors::new(0.02, 0.005);
        factors.utilization_premium_rate = 0.5; // 50% premium when fully utilized

        let snap_idle = base_snapshot(0.0, 2 * 1024 * 1024 * 1024);
        let snap_full = base_snapshot(100.0, 2 * 1024 * 1024 * 1024);

        let cost_idle = snap_idle.estimate_monthly_cost(&factors, 4);
        let cost_full = snap_full.estimate_monthly_cost(&factors, 4);

        assert!((cost_idle - 65.7).abs() < f64::EPSILON);
        assert!((cost_full - (65.7 * 1.5)).abs() < f64::EPSILON);
    }
}
