//! Cost estimation module.
//!
//! # Abstract
//! Estimates the hourly cost of running a micro-VM based on its configuration.
//!
//! # The Hero's Journey
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
//! let factors = PricingFactors::new(0.01, 0.005);
//! let cost_per_hour = config.estimate_cost(&factors);
//! assert!(cost_per_hour > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for cost estimation.
///
/// Holds the hourly rates for CPU cores and RAM to compute total run cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per hour for one virtual CPU.
    pub cpu_hourly_rate: f64,
    /// Cost per hour for one Gigabyte (1024 MiB) of RAM.
    pub ram_hourly_rate_per_gb: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` configuration.
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_hourly_rate_per_gb: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_hourly_rate_per_gb,
        }
    }
}

/// Trait to estimate the cost of running a micro-VM.
pub trait CostEstimator {
    /// Estimates the cost per hour of running the configuration based on given rates.
    fn estimate_cost(&self, factors: &PricingFactors) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, factors: &PricingFactors) -> f64 {
        let cpu_cost = f64::from(self.cpus) * factors.cpu_hourly_rate;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * factors.ram_hourly_rate_per_gb;
        cpu_cost + ram_cost
    }
}

#[cfg(test)]
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

        let factors = PricingFactors::new(0.02, 0.005);
        let cost = config.estimate_cost(&factors);
        // 2 CPUs * 0.02 + 2 GB RAM * 0.005 = 0.04 + 0.01 = 0.05
        assert!((cost - 0.05).abs() < f64::EPSILON);
    }
}
