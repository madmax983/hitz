//! Billing cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the absolute resource
//! utilization of a system (`MetricsSnapshot`) with configurable billing rates
//! to estimate the real-time cost of the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, BillingProfile, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 100.0, per_core: vec![100.0, 100.0], load_avg: [1.0, 1.0, 1.0] },
//!     memory: MemoryMetrics { total_bytes: 1024 * 1024 * 1024, used_bytes: 512 * 1024 * 1024, free_bytes: 512 * 1024 * 1024, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // Using a profile charging $0.05 per vCPU/hour and $0.01 per GB RAM/hour
//! let profile = BillingProfile::new(0.05, 0.01);
//! let cost = snap.estimate_cost(&profile, 2); // 2 CPUs
//! assert!(cost > 0.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable billing rates for the data center region.
///
/// # Abstract
/// This struct holds the assumptions needed to translate resource consumption
/// into dollar cost.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::BillingProfile;
///
/// let profile = BillingProfile::new(0.05, 0.01);
/// assert_eq!(profile.cpu_hourly_rate, 0.05);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingProfile {
    /// Cost per vCPU per hour in USD.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_hourly_rate_per_gb: f64,
}

impl BillingProfile {
    /// Creates a new `BillingProfile` with specified rates.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::BillingProfile;
    ///
    /// let profile = BillingProfile::new(0.05, 0.01);
    /// assert_eq!(profile.ram_hourly_rate_per_gb, 0.01);
    /// ```
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_hourly_rate_per_gb: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_hourly_rate_per_gb,
        }
    }
}

/// Trait for objects that can estimate their cost.
pub trait CostEstimator {
    /// Estimates the current rate of cost in USD per second.
    fn estimate_cost(&self, profile: &BillingProfile, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_cost(&self, profile: &BillingProfile, vcpus: u32) -> f64 {
        let vcpus_f64 = f64::from(vcpus);
        let cpu_cost_per_hour = vcpus_f64 * profile.cpu_hourly_rate;

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost_per_hour = ram_gb * profile.ram_hourly_rate_per_gb;

        let total_cost_per_hour = cpu_cost_per_hour + ram_cost_per_hour;

        total_cost_per_hour / 3600.0
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(ram_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
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
    fn test_cost_estimator() {
        let profile = BillingProfile::new(0.05, 0.01);
        // 2 GB RAM
        let snap = base_snapshot(2 * 1024 * 1024 * 1024);

        // 4 vcpus * 0.05 = 0.20
        // 2 GB * 0.01 = 0.02
        // total = 0.22 per hour
        // per sec = 0.22 / 3600 = 0.000061111...

        let cost = snap.estimate_cost(&profile, 4);
        assert!((cost - (0.22 / 3600.0)).abs() < f64::EPSILON);
    }
}
