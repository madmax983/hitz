//! Cost estimation module.
//!
//! # Abstract
//! This module provides a way to estimate the monthly cost of a VM
//! based on its configuration (`VmConfig`) and a set of pricing factors.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingFactors, VmConfig, GuestAgentMode};
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
//! let factors = PricingFactors {
//!     cost_per_cpu_month: 5.0,
//!     cost_per_gb_ram_month: 2.0,
//! };
//!
//! let monthly_cost = config.estimate_monthly_cost(&factors);
//! assert_eq!(monthly_cost, 22.0); // 4 * 5.0 + 1 * 2.0
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Monthly cost per vCPU.
    pub cost_per_cpu_month: f64,
    /// Monthly cost per GB of RAM.
    pub cost_per_gb_ram_month: f64,
}

/// Trait to estimate the monthly cost of a resource.
pub trait CostEstimator {
    /// Estimates the monthly cost based on pricing factors.
    fn estimate_monthly_cost(&self, factors: &PricingFactors) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_monthly_cost(&self, factors: &PricingFactors) -> f64 {
        let cpu_cost = f64::from(self.cpus) * factors.cost_per_cpu_month;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * factors.cost_per_gb_ram_month;
        cpu_cost + ram_cost
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
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
            ram_mib: 2048,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let factors = PricingFactors {
            cost_per_cpu_month: 10.0,
            cost_per_gb_ram_month: 5.0,
        };

        let cost = config.estimate_monthly_cost(&factors);
        assert_eq!(cost, 30.0); // 2*10 + 2*5
    }
}
