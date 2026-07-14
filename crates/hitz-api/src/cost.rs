//! Cloud Billing Estimator Module
//!
//! # Abstract
//! This module provides a way to estimate the financial cost of running a VM
//! configuration (`VmConfig`), translating resources into standard monthly or
//! hourly cloud billing estimates.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, CostEstimator, PricingModel, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 2048,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! // Pricing: $0.05 per vCPU/hour, $0.01 per GB RAM/hour
//! let pricing = PricingModel {
//!     cpu_hourly_rate: 0.05,
//!     ram_gb_hourly_rate: 0.01,
//! };
//!
//! let cost = config.estimate_cost(&pricing);
//! assert!(cost.hourly_cost > 0.2); // 4 CPUs * 0.05 = 0.2 + 2GB * 0.01 = 0.02 -> 0.22
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Represents a pricing model for computing resource costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per hour for a single vCPU.
    pub cpu_hourly_rate: f64,
    /// Cost per hour for 1 GiB of RAM.
    pub ram_gb_hourly_rate: f64,
}

impl Default for PricingModel {
    fn default() -> Self {
        Self {
            cpu_hourly_rate: 0.04,
            ram_gb_hourly_rate: 0.005,
        }
    }
}

/// The estimated financial cost of running a workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimatedCost {
    /// Total estimated cost per hour.
    pub hourly_cost: f64,
    /// Total estimated cost per day (24 hours).
    pub daily_cost: f64,
    /// Total estimated cost per month (730 hours average).
    pub monthly_cost: f64,
}

/// Trait to estimate financial cost.
pub trait CostEstimator {
    /// Calculates the estimated cost based on the given pricing model.
    fn estimate_cost(&self, pricing: &PricingModel) -> EstimatedCost;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, pricing: &PricingModel) -> EstimatedCost {
        let cpu_cost = f64::from(self.cpus) * pricing.cpu_hourly_rate;
        // ram_mib to GiB
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.ram_gb_hourly_rate;

        let hourly_cost = cpu_cost + ram_cost;

        EstimatedCost {
            hourly_cost,
            daily_cost: hourly_cost * 24.0,
            monthly_cost: hourly_cost * 730.0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
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

    #[test]
    fn test_estimate_cost() {
        let config = test_config(4, 2048);
        let pricing = PricingModel {
            cpu_hourly_rate: 0.05,
            ram_gb_hourly_rate: 0.01,
        };

        let cost = config.estimate_cost(&pricing);

        // 4 * 0.05 = 0.20
        // 2048 / 1024 = 2.0 * 0.01 = 0.02
        // Total hourly = 0.22
        assert!((cost.hourly_cost - 0.22).abs() < f64::EPSILON);
        assert!((cost.daily_cost - (0.22 * 24.0)).abs() < f64::EPSILON);
        assert!((cost.monthly_cost - (0.22 * 730.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_zero_resources() {
        let config = test_config(0, 0);
        let pricing = PricingModel::default();
        let cost = config.estimate_cost(&pricing);
        assert!((cost.hourly_cost - 0.0).abs() < f64::EPSILON);
    }
}
