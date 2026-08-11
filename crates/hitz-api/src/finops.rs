//! `FinOps` module for estimating VM costs.
//!
//! # Abstract
//! Calculates estimated monthly costs for a VM configuration based on pricing factors.

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Pricing factors for cost estimation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per vCPU per month in USD.
    pub cpu_monthly_cost: f64,
    /// Cost per GB of RAM per month in USD.
    pub ram_monthly_cost_per_gb: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` with default public cloud estimates.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cpu_monthly_cost: 2.0,
            ram_monthly_cost_per_gb: 1.5,
        }
    }
}

impl Default for PricingFactors {
    fn default() -> Self {
        Self::new()
    }
}

/// The computed estimated monthly cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Total estimated cost per month in USD.
    pub total_monthly_usd: f64,
    /// Breakdown of the cost.
    pub breakdown: String,
}

/// Trait to estimate VM costs.
pub trait CostEstimator {
    /// Estimates the monthly cost based on pricing factors.
    fn estimate_cost(&self, factors: &PricingFactors) -> CostEstimate;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, factors: &PricingFactors) -> CostEstimate {
        let cpu_cost = f64::from(self.cpus) * factors.cpu_monthly_cost;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * factors.ram_monthly_cost_per_gb;
        let total = cpu_cost + ram_cost;

        CostEstimate {
            total_monthly_usd: total,
            breakdown: format!("CPU: ${cpu_cost:.2}, RAM: ${ram_cost:.2}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_cost_estimator() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048, // 2 GB
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let factors = PricingFactors::new();
        let estimate = config.estimate_cost(&factors);

        // 4 CPUs * 2.0 = 8.0
        // 2 GB * 1.5 = 3.0
        // Total = 11.0
        assert!((estimate.total_monthly_usd - 11.0).abs() < f64::EPSILON);
        assert!(estimate.breakdown.contains("CPU: $8.00"));
        assert!(estimate.breakdown.contains("RAM: $3.00"));
    }
}
