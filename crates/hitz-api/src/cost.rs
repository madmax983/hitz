//! Financial cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that estimates the hourly
//! cost of running a micro-VM based on its resource allocation and optional
//! carbon taxes.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 50.0,
//!         per_core: vec![50.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 0,
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
//! // Define pricing: $0.05/vCPU/hr, $0.01/GB/hr, and a carbon tax of $50/tonne
//! let pricing = PricingFactors::new(0.05, 0.01).with_carbon_tax(50.0);
//! let hourly_cost = snap.estimate_cost(&pricing, 4, 10.5); // 4 CPUs, 10.5 mgCO2/s
//! assert!(hourly_cost > 0.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for cloud resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per virtual CPU core per hour (in dollars).
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour (in dollars).
    pub ram_gb_hourly_rate: f64,
    /// Optional carbon tax rate in dollars per metric tonne of `CO2eq`.
    pub carbon_tax_per_tonne: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` profile with base rates.
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_gb_hourly_rate,
            carbon_tax_per_tonne: 0.0,
        }
    }

    /// Sets the carbon tax rate for the pricing model.
    #[must_use]
    pub const fn with_carbon_tax(mut self, rate: f64) -> Self {
        self.carbon_tax_per_tonne = rate;
        self
    }
}

/// Trait for estimating the financial cost of running a VM.
pub trait CostEstimator {
    /// Estimates the current hourly cost in dollars.
    ///
    /// Requires the pricing factors, the number of allocated vCPUs, and
    /// optionally the current carbon emission rate in mg CO2/sec to compute
    /// the carbon tax.
    fn estimate_cost(&self, factors: &PricingFactors, vcpus: u32, emissions_mg_per_sec: f64)
    -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_cost(
        &self,
        factors: &PricingFactors,
        vcpus: u32,
        emissions_mg_per_sec: f64,
    ) -> f64 {
        let vcpus_f64 = f64::from(vcpus);
        let cpu_cost = vcpus_f64 * factors.cpu_hourly_rate;

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = ram_gb * factors.ram_gb_hourly_rate;

        // Calculate carbon tax
        // emissions_mg_per_sec -> mg per hour: * 3600
        // mg per hour -> tonnes per hour: / 1,000,000,000
        let emissions_tonnes_per_hour = (emissions_mg_per_sec * 3600.0) / 1_000_000_000.0;
        let carbon_tax = emissions_tonnes_per_hour * factors.carbon_tax_per_tonne;

        cpu_cost + ram_cost + carbon_tax
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(ram_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0],
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
    fn test_cost_estimator_base_rates() {
        let factors = PricingFactors::new(0.05, 0.01);
        let snap = base_snapshot(4 * 1024 * 1024 * 1024); // 4 GB RAM

        // CPU cost: 2 vcpus * $0.05 = $0.10
        // RAM cost: 4 GB * $0.01 = $0.04
        // Total cost = $0.14
        let cost = snap.estimate_cost(&factors, 2, 0.0);
        assert!(
            (cost - 0.14).abs() < f64::EPSILON,
            "Expected 0.14, got {cost}"
        );
    }

    #[test]
    fn test_cost_estimator_with_carbon_tax() {
        let factors = PricingFactors::new(0.05, 0.01).with_carbon_tax(100.0);
        let snap = base_snapshot(4 * 1024 * 1024 * 1024); // 4 GB RAM

        // Base cost: $0.14
        // Emission: 50000.0 mg/sec = 180,000,000 mg/hr = 0.18 tonnes/hr
        // Carbon tax: 0.18 * $100 = $18.0
        // Total cost: $18.14
        let cost = snap.estimate_cost(&factors, 2, 50000.0);
        assert!(
            (cost - 18.14).abs() < f64::EPSILON,
            "Expected 18.14, got {cost}"
        );
    }
}
