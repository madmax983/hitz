//! `FinOps` cost estimator for micro-VMs.
//!
//! # Abstract
//! This module estimates the financial cost of running a micro-VM,
//! bridging the gap between technical configuration and business impact.
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
//!     ram_mib: 2048,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let pricing = PricingModel::new(0.02, 0.005); // $0.02/vCPU/hr, $0.005/GB/hr
//! let cost = config.estimate_monthly_cost(&pricing);
//! assert!(cost > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing model for a cloud or on-prem environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour in USD.
    pub vcpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_gb_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel`.
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_gb_hourly_rate,
        }
    }
}

/// Trait for objects that can estimate their financial cost.
pub trait CostEstimator {
    /// Estimates the monthly cost (assuming 730 hours/month) in USD based on the given pricing model.
    fn estimate_monthly_cost(&self, pricing: &PricingModel) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_monthly_cost(&self, pricing: &PricingModel) -> f64 {
        let vcpus = f64::from(self.cpus);
        let ram_gb = f64::from(self.ram_mib) / 1024.0;

        let hourly_cost = vcpus.mul_add(
            pricing.vcpu_hourly_rate,
            ram_gb * pricing.ram_gb_hourly_rate,
        );

        // Average hours in a month: 730
        hourly_cost * 730.0
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
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
            ram_mib: 4096, // 4 GB
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let pricing = PricingModel::new(0.01, 0.005);
        // hourly: (2 * 0.01) + (4 * 0.005) = 0.02 + 0.02 = 0.04
        // monthly: 0.04 * 730 = 29.2
        let cost = config.estimate_monthly_cost(&pricing);
        assert!((cost - 29.2).abs() < 1e-10);
    }
}
