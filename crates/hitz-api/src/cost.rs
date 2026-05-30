//! Cost estimator module.
//!
//! # Abstract
//! Calculates the cost of running a VM based on configuration and pricing model.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingModel, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let pricing = PricingModel {
//!     cpu_hourly_rate: 0.05,
//!     ram_gb_hourly_rate: 0.01,
//! };
//!
//! let cost = config.estimate_cost(&pricing);
//! assert!(cost.hourly > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Defines the pricing rates for resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour
    pub ram_gb_hourly_rate: f64,
}

/// The estimated cost of running a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmCost {
    /// Estimated hourly cost.
    pub hourly: f64,
    /// Estimated monthly cost (assuming 730 hours).
    pub monthly: f64,
}

/// Trait for objects that can evaluate configuration against a pricing model to estimate cost.
pub trait CostEstimator {
    /// Estimates the cost of the configuration given a pricing model.
    fn estimate_cost(&self, pricing: &PricingModel) -> VmCost;
}

impl CostEstimator for VmConfig {
    fn estimate_cost(&self, pricing: &PricingModel) -> VmCost {
        let cpu_cost = f64::from(self.cpus) * pricing.cpu_hourly_rate;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.ram_gb_hourly_rate;

        let hourly = cpu_cost + ram_cost;
        let monthly = hourly * 730.0; // 730 hours in a month

        VmCost { hourly, monthly }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_cost_estimation() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048, // 2 GB
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let pricing = PricingModel {
            cpu_hourly_rate: 0.05,
            ram_gb_hourly_rate: 0.01,
        };

        let cost = config.estimate_cost(&pricing);

        // Expected: 2 cpus * 0.05 + 2 GB * 0.01 = 0.10 + 0.02 = 0.12
        assert!((cost.hourly - 0.12).abs() < 1e-10);
        assert!((cost.monthly - (0.12 * 730.0)).abs() < 1e-10);
    }
}
