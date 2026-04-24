//! Cost estimator for micro-VM workloads.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the absolute resource
//! utilization of a system (`MetricsSnapshot`) with configurable pricing factors
//! to estimate the real-time financial cost of running the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Assume we have a snapshot showing resource usage
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 100.0,
//!         per_core: vec![100.0, 100.0],
//!         load_avg: [1.0, 1.0, 1.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
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
//! // Configure costs: $0.05 per vCPU/hr, $0.01 per GB RAM/hr
//! let pricing = PricingFactors::new(0.05, 0.01);
//!
//! // Estimate the cost per hour for a 2 vCPU VM
//! let cost_per_hour = snap.estimate_cost(&pricing, 2);
//! assert!(cost_per_hour > 0.1);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for cloud resources.
///
/// # Abstract
/// This struct holds the rates needed to translate allocated and utilized
/// resources into a monetary cost (e.g., USD per hour).
///
/// # Details
/// It includes base costs for allocated resources as well as optional
/// premiums for high utilization.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::PricingFactors;
///
/// let pricing = PricingFactors::new(0.04, 0.005);
/// assert_eq!(pricing.cpu_per_hour, 0.04);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per vCPU per hour.
    pub cpu_per_hour: f64,
    /// Cost per GB of RAM per hour.
    pub ram_gb_per_hour: f64,
    /// Additional premium cost per hour if CPU utilization is above 80%.
    pub high_utilization_premium: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` profile with basic rates.
    ///
    /// # Abstract
    /// A quick factory method to instantiate pricing factors with standard rates.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::PricingFactors;
    ///
    /// let pricing = PricingFactors::new(0.05, 0.01);
    /// assert_eq!(pricing.cpu_per_hour, 0.05);
    /// assert_eq!(pricing.high_utilization_premium, 0.0);
    /// ```
    #[must_use]
    pub const fn new(cpu_per_hour: f64, ram_gb_per_hour: f64) -> Self {
        Self {
            cpu_per_hour,
            ram_gb_per_hour,
            high_utilization_premium: 0.0,
        }
    }

    /// Sets the high utilization premium.
    #[must_use]
    pub const fn with_premium(mut self, premium: f64) -> Self {
        self.high_utilization_premium = premium;
        self
    }
}

/// Trait for objects that can estimate their running cost.
///
/// # Abstract
/// This trait defines the capability to estimate the financial cost
/// of a micro-VM based on its current resource allocation and utilization.
pub trait CostEstimator {
    /// Estimates the current running cost in monetary units per hour.
    ///
    /// # Abstract
    /// Applies a pricing model to a snapshot to estimate the hourly cost.
    ///
    /// # Details
    /// Requires the number of allocated virtual CPUs (`vcpus`) to calculate
    /// the base compute cost.
    fn estimate_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64 {
        let vcpus_f64 = f64::from(vcpus);
        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        let base_cost = (vcpus_f64 * factors.cpu_per_hour) + (ram_gb * factors.ram_gb_per_hour);

        let mut total_cost = base_cost;

        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0);
        if cpu_utilization > 80.0 {
            total_cost += factors.high_utilization_premium;
        }

        total_cost
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu_pct: f32, ram_bytes: u64) -> MetricsSnapshot {
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
        let factors = PricingFactors::new(0.05, 0.01); // $0.05 per vcpu, $0.01 per GB
        let snap = dummy_snapshot(50.0, 4 * 1024 * 1024 * 1024); // 50% CPU, 4GB RAM

        // 2 vcpus * 0.05 = 0.10
        // 4 GB * 0.01 = 0.04
        // Total = 0.14
        let cost = snap.estimate_cost(&factors, 2);
        assert!(
            (cost - 0.14).abs() < f64::EPSILON,
            "Expected 0.14, got {cost}"
        );
    }

    #[test]
    fn test_cost_estimator_premium() {
        let factors = PricingFactors::new(0.05, 0.01).with_premium(0.02);
        let snap = dummy_snapshot(90.0, 4 * 1024 * 1024 * 1024); // 90% CPU, 4GB RAM

        // 2 vcpus * 0.05 = 0.10
        // 4 GB * 0.01 = 0.04
        // Premium = 0.02
        // Total = 0.16
        let cost = snap.estimate_cost(&factors, 2);
        assert!(
            (cost - 0.16).abs() < f64::EPSILON,
            "Expected 0.16, got {cost}"
        );
    }
}
