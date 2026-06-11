//! Cloud pricing cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects a VM's configuration
//! (`VmConfig`) with a pricing model to estimate its monthly running cost.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, CloudPricing, VmConfig, GuestAgentMode};
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
//! // Using AWS Fargate-like pricing ($0.04048/vCPU, $0.004445/GB)
//! let pricing = CloudPricing::new(0.04048, 0.004445);
//! let monthly_cost = config.estimate_monthly_cost(&pricing);
//! // 4 vCPUs + 2 GB RAM -> ($0.16192 + $0.00889) * 730 = $124.6913
//! assert!((monthly_cost - 124.6913).abs() < 1e-4);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing model for a cloud provider.
///
/// # Abstract
/// This struct holds hourly pricing rates for vCPUs and RAM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPricing {
    /// Hourly price per vCPU.
    pub hourly_price_per_vcpu: f64,
    /// Hourly price per GB of RAM.
    pub hourly_price_per_gb_ram: f64,
}

impl CloudPricing {
    /// Creates a new `CloudPricing` instance.
    #[must_use]
    pub const fn new(hourly_price_per_vcpu: f64, hourly_price_per_gb_ram: f64) -> Self {
        Self {
            hourly_price_per_vcpu,
            hourly_price_per_gb_ram,
        }
    }
}

/// Trait for objects that can estimate their running cost.
pub trait CostEstimator {
    /// Estimates the monthly running cost assuming 730 hours in a month.
    fn estimate_monthly_cost(&self, pricing: &CloudPricing) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_monthly_cost(&self, pricing: &CloudPricing) -> f64 {
        let vcpus_f64 = f64::from(self.cpus);
        let ram_gb = f64::from(self.ram_mib) / 1024.0;

        let hourly_cost = (vcpus_f64 * pricing.hourly_price_per_vcpu)
            + (ram_gb * pricing.hourly_price_per_gb_ram);

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
            ram_mib: 1024,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let pricing = CloudPricing::new(0.01, 0.005);
        // (2 * 0.01 + 1 * 0.005) * 730 = 0.025 * 730 = 18.25
        let cost = config.estimate_monthly_cost(&pricing);
        assert!((cost - 18.25).abs() < f64::EPSILON);
    }
}
