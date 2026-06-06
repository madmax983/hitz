//! Financial cost estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the resource
//! configuration of a VM (`VmConfig`) with a configurable pricing model
//! to estimate the running cost of the VM over time.
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
//! // Using a pricing model: $0.05 per vCPU/hour, $0.01 per GB RAM/hour
//! let pricing = PricingModel::new(0.05, 0.01);
//! let cost_per_hour = config.estimate_cost(&pricing);
//! println!("Estimated cost: ${:.2}/hour", cost_per_hour);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing model for VM resources.
///
/// # Abstract
/// Defines the hourly cost rates for compute (vCPU) and memory (RAM) resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per hour for a single vCPU in USD.
    pub vcpu_hourly_rate: f64,
    /// Cost per hour for 1 GB (1024 MiB) of RAM in USD.
    pub ram_gb_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel` with the given hourly rates.
    #[must_use]
    pub const fn new(vcpu_hourly_rate: f64, ram_gb_hourly_rate: f64) -> Self {
        Self {
            vcpu_hourly_rate,
            ram_gb_hourly_rate,
        }
    }
}

/// Trait for objects that can estimate their financial running cost.
///
/// # Abstract
/// Provides a standard way to project the financial cost of running a micro-VM
/// configuration based on a specific pricing model.
pub trait CostEstimator {
    /// Estimates the cost per hour of running the VM in USD.
    fn estimate_cost(&self, model: &PricingModel) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, model: &PricingModel) -> f64 {
        let cpu_cost = f64::from(self.cpus) * model.vcpu_hourly_rate;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * model.ram_gb_hourly_rate;
        cpu_cost + ram_cost
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
            ram_mib: 2048, // 2 GB
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let model = PricingModel::new(0.05, 0.01);
        // Expected: 4 CPUs * 0.05 + 2 GB * 0.01 = 0.20 + 0.02 = 0.22
        let cost = config.estimate_cost(&model);
        assert!(
            (cost - 0.22).abs() < f64::EPSILON,
            "Expected 0.22, got {cost}",
        );
    }
}
