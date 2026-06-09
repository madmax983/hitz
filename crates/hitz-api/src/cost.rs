//! Cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the absolute resource
//! utilization of a system (`MetricsSnapshot`) with configurable pricing factors
//! to estimate the real-time running cost of the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Assume we have a snapshot showing 50% CPU usage
//! // let snap: MetricsSnapshot = ...;
//! //
//! // // Using a cloud provider with $0.05/vCPU/hour and $0.01/GB/hour
//! // let factors = PricingFactors::new(0.05, 0.01);
//! // let cost = snap.estimate_cost(&factors, 4); // 4 CPUs
//! // println!("Current estimated cost: ${:.4}/hour", cost);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for a cloud provider or on-prem environment.
///
/// # Abstract
/// This struct holds the assumptions needed to translate allocated resources
/// and utilization into estimated costs in a given currency per hour.
///
/// # Details
/// It includes hourly rates for vCPU and RAM, plus an optional utilization
/// multiplier if costs scale with active usage rather than just allocation.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::PricingFactors;
///
/// // Create a profile for standard cloud pricing
/// let profile = PricingFactors::new(0.05, 0.01);
/// assert_eq!(profile.vcpu_hourly_rate, 0.05);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Hourly cost per allocated vCPU core.
    pub vcpu_hourly_rate: f64,
    /// Hourly cost per allocated GB of RAM.
    pub ram_hourly_rate_per_gb: f64,
    /// Whether to adjust the vCPU cost based on active utilization (e.g., serverless model).
    pub utilization_based: bool,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` profile with standard flat-rate assumptions.
    ///
    /// # Abstract
    /// A quick factory method to instantiate pricing factors assuming fixed allocation costs.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::PricingFactors;
    ///
    /// let factors = PricingFactors::new(0.04, 0.005);
    /// assert_eq!(factors.vcpu_hourly_rate, 0.04);
    /// assert_eq!(factors.ram_hourly_rate_per_gb, 0.005);
    /// assert!(!factors.utilization_based);
    /// ```
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_hourly_rate_per_gb: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_hourly_rate_per_gb,
            utilization_based: false,
        }
    }
}

/// Trait for objects that can estimate their running cost.
///
/// # Abstract
/// This trait defines the capability to estimate the real-time running cost
/// of a micro-VM based on its current resource allocation and utilization.
pub trait CostEstimator {
    /// Estimates the current rate of cost in currency units per hour.
    ///
    /// # Abstract
    /// Applies a pricing model to a utilization snapshot to estimate
    /// the instantaneous cost rate.
    ///
    /// # Details
    /// Requires the number of allocated virtual CPUs (`vcpus`) because `MetricsSnapshot`
    /// does not intrinsically know the total core count.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{CostEstimator, PricingFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics {
    ///         total_pct: 100.0,
    ///         per_core: vec![100.0, 100.0],
    ///         load_avg: [1.0, 1.0, 1.0],
    ///     },
    ///     memory: MemoryMetrics {
    ///         total_bytes: 1024 * 1024 * 1024, // 1 GB
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
    /// let factors = PricingFactors::new(0.10, 0.02);
    /// let cost = snap.estimate_cost(&factors, 2);
    /// assert!(cost > 0.0);
    /// ```
    fn estimate_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, factors: &PricingFactors, vcpus: u32) -> f64 {
        let vcpus_f64 = f64::from(vcpus);

        let cpu_cost = if factors.utilization_based {
            let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
            vcpus_f64 * factors.vcpu_hourly_rate * cpu_utilization
        } else {
            vcpus_f64 * factors.vcpu_hourly_rate
        };

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = ram_gb * factors.ram_hourly_rate_per_gb;

        cpu_cost + ram_cost
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
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
    fn test_cost_estimator_flat_rate() {
        let factors = PricingFactors::new(0.05, 0.01);
        let snap = base_snapshot(50.0, 2 * 1024 * 1024 * 1024); // 50% CPU, 2GB RAM

        let cost = snap.estimate_cost(&factors, 4);

        // Expected CPU Cost: 4 * 0.05 = 0.20
        // Expected RAM Cost: 2 * 0.01 = 0.02
        // Total: 0.22
        assert!(
            (cost - 0.22).abs() < f64::EPSILON,
            "Expected 0.22, got {cost}"
        );
    }

    #[test]
    fn test_cost_estimator_utilization_based() {
        let mut factors = PricingFactors::new(0.05, 0.01);
        factors.utilization_based = true;
        let snap = base_snapshot(50.0, 2 * 1024 * 1024 * 1024); // 50% CPU, 2GB RAM

        let cost = snap.estimate_cost(&factors, 4);

        // Expected CPU Cost: 4 * 0.05 * 0.50 = 0.10
        // Expected RAM Cost: 2 * 0.01 = 0.02
        // Total: 0.12
        assert!(
            (cost - 0.12).abs() < f64::EPSILON,
            "Expected 0.12, got {cost}"
        );
    }
}
