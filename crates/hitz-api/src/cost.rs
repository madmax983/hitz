//! Cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that uses the absolute resource
//! utilization of a system (`MetricsSnapshot`) and an hourly `BillingRate` to
//! calculate the real-time financial cost (in fractions of a cent per second) of running the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, BillingRate, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Assume we have a snapshot
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
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
//! //
//! // Using a billing rate of $0.05 per vCPU hour and $0.01 per GB RAM hour
//! let rate = BillingRate::new(0.05, 0.01);
//! let cost_per_sec = snap.estimate_cost(&rate, 4); // 4 CPUs
//! // println!("Current cost rate: ${:.6}/sec", cost_per_sec);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Hourly billing rates for VM resources.
///
/// # Abstract
/// Defines the pricing model per vCPU and per GB of RAM, expressed in a base currency (e.g. USD).
///
/// ## Examples
///
/// ```rust
/// use hitz_api::BillingRate;
///
/// let rate = BillingRate::new(0.05, 0.01);
/// assert_eq!(rate.usd_per_vcpu_hour, 0.05);
/// assert_eq!(rate.usd_per_gb_ram_hour, 0.01);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingRate {
    /// Cost per virtual CPU per hour.
    pub usd_per_vcpu_hour: f64,
    /// Cost per Gigabyte of RAM per hour.
    pub usd_per_gb_ram_hour: f64,
}

impl BillingRate {
    /// Creates a new `BillingRate` configuration.
    #[must_use]
    pub const fn new(usd_per_vcpu_hour: f64, usd_per_gb_ram_hour: f64) -> Self {
        Self {
            usd_per_vcpu_hour,
            usd_per_gb_ram_hour,
        }
    }
}

/// Trait for objects that can estimate their real-time operating cost.
///
/// # Abstract
/// Uses resource allocations to output an estimated instantaneous cost rate.
pub trait CostEstimator {
    /// Estimates the current rate of cost in currency units per second (e.g., USD/sec).
    ///
    /// # Abstract
    /// Multiplies provisioned vCPUs and total memory by the billing rate to calculate
    /// the real-time fractional cost.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{CostEstimator, BillingRate, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics {
    ///         total_pct: 10.0,
    ///         per_core: vec![10.0],
    ///         load_avg: [0.1, 0.1, 0.1],
    ///     },
    ///     memory: MemoryMetrics {
    ///         total_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
    ///         used_bytes: 512 * 1024 * 1024,
    ///         free_bytes: 512 * 1024 * 1024,
    ///         buffers_bytes: 0,
    ///         cached_bytes: 0,
    ///         swap_total: 0,
    ///         swap_used: 0,
    ///     },
    ///     disks: vec![],
    ///     networks: vec![],
    ///     processes: vec![],
    /// };
    ///
    /// let rate = BillingRate::new(0.04, 0.005);
    /// let cost = snap.estimate_cost(&rate, 2);
    /// assert!(cost > 0.0);
    /// ```
    fn estimate_cost(&self, rate: &BillingRate, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, rate: &BillingRate, vcpus: u32) -> f64 {
        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        let cpu_cost_per_hour = f64::from(vcpus) * rate.usd_per_vcpu_hour;
        let ram_cost_per_hour = ram_gb * rate.usd_per_gb_ram_hour;

        let total_cost_per_hour = cpu_cost_per_hour + ram_cost_per_hour;

        // Convert to cost per second
        total_cost_per_hour / 3600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_cost_estimator() {
        let rate = BillingRate::new(0.05, 0.01);
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 0.0,
                per_core: vec![0.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 4 * 1024 * 1024 * 1024, // 4GB
                used_bytes: 0,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        // 2 vCPUs = $0.10/hr. 4GB RAM = $0.04/hr. Total = $0.14/hr.
        // Per second = 0.14 / 3600 = 0.000038888...
        let expected_per_sec = 0.14 / 3600.0;
        let actual_per_sec = snap.estimate_cost(&rate, 2);

        assert!((expected_per_sec - actual_per_sec).abs() < f64::EPSILON);
    }
}
