//! \`FinOps\` Cost Estimator Module
//!
//! # Abstract
//! Converts a `VmConfig` into a monthly financial cost estimation based on a provided pricing model.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, CostEstimator, PricingModel, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 2048,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let model = PricingModel { vcpu_hourly_rate: 0.04, ram_gb_hourly_rate: 0.005, base_hourly_rate: 0.01 };
//! let monthly_cost = config.estimate_monthly_cost(&model);
//! assert!(monthly_cost > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// A pricing model for estimating VM costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Hourly rate per vCPU.
    pub vcpu_hourly_rate: f64,
    /// Hourly rate per GB of RAM.
    pub ram_gb_hourly_rate: f64,
    /// Base hourly rate for running a VM.
    pub base_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new pricing model with standard defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vcpu_hourly_rate: 0.02,
            ram_gb_hourly_rate: 0.005,
            base_hourly_rate: 0.01,
        }
    }
}

impl Default for PricingModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait to estimate financial costs for a configuration.
pub trait CostEstimator {
    /// Estimates the monthly cost (assuming 730 hours/month).
    fn estimate_monthly_cost(&self, model: &PricingModel) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::suboptimal_flops, clippy::cast_precision_loss)]
    fn estimate_monthly_cost(&self, model: &PricingModel) -> f64 {
        let hours_per_month = 730.0;
        let vcpus = f64::from(self.cpus);
        let ram_gb = f64::from(self.ram_mib) / 1024.0;

        let hourly_cost = model.base_hourly_rate
            + (vcpus * model.vcpu_hourly_rate)
            + (ram_gb * model.ram_gb_hourly_rate);

        hourly_cost * hours_per_month
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
        let model = PricingModel {
            vcpu_hourly_rate: 0.05,
            ram_gb_hourly_rate: 0.01,
            base_hourly_rate: 0.02,
        };
        let config = VmConfig {
            kernel_path: PathBuf::from(""),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let cost = config.estimate_monthly_cost(&model);
        // (0.02 + 2*0.05 + 1*0.01) * 730 = 0.13 * 730 = 94.9000...
        assert!((cost - 94.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_pricing_model() {
        let model = PricingModel::default();
        assert!((model.vcpu_hourly_rate - 0.02).abs() < f64::EPSILON);
    }
}
