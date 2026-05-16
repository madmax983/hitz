//! Cloud billing estimation for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that translates the static
//! configuration (`VmConfig`) into an estimated hourly cost based on a
//! pricing profile.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, BillingProfile, VmConfig, GuestAgentMode};
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
//! let profile = BillingProfile::aws_fargate();
//! let cost = config.estimate_cost(&profile);
//!
//! assert!(cost.hourly_total > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing profile for cloud cost estimation.
///
/// # Abstract
/// Holds the hourly cost rates for CPU cores and RAM to estimate the
/// financial cost of running a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingProfile {
    /// Cost per CPU core per hour in USD.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_gb_hourly_rate: f64,
    /// Base fixed cost per hour (e.g., for IP addresses or base platform fees).
    pub base_hourly_rate: f64,
}

impl BillingProfile {
    /// Creates a new `BillingProfile`.
    #[must_use]
    pub const fn new(cpu_rate: f64, ram_rate: f64, base_rate: f64) -> Self {
        Self {
            cpu_hourly_rate: cpu_rate,
            ram_gb_hourly_rate: ram_rate,
            base_hourly_rate: base_rate,
        }
    }

    /// Pre-configured profile resembling AWS Fargate pricing (us-east-1).
    #[must_use]
    pub const fn aws_fargate() -> Self {
        Self {
            cpu_hourly_rate: 0.04048,
            ram_gb_hourly_rate: 0.004_445,
            base_hourly_rate: 0.0,
        }
    }
}

/// A detailed breakdown of estimated VM costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmCost {
    /// Total cost per hour in USD.
    pub hourly_total: f64,
    /// Monthly cost assuming 730 hours/month.
    pub monthly_total: f64,
}

/// Trait for objects that can estimate their financial cost.
pub trait CostEstimator {
    /// Estimates the cost of running based on a `BillingProfile`.
    fn estimate_cost(&self, profile: &BillingProfile) -> VmCost;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, profile: &BillingProfile) -> VmCost {
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let base_and_cpu =
            f64::from(self.cpus).mul_add(profile.cpu_hourly_rate, profile.base_hourly_rate);
        let hourly_total = ram_gb.mul_add(profile.ram_gb_hourly_rate, base_and_cpu);

        VmCost {
            hourly_total,
            monthly_total: hourly_total * 730.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    #[test]
    fn test_billing_profile_creation() {
        let profile = BillingProfile::new(0.05, 0.01, 0.0);
        assert!((profile.cpu_hourly_rate - 0.05).abs() < f64::EPSILON);
        assert!((profile.ram_gb_hourly_rate - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn test_aws_fargate_profile() {
        let profile = BillingProfile::aws_fargate();
        assert!(profile.cpu_hourly_rate > 0.0);
    }

    #[test]
    fn test_vm_config_cost_estimation() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        // 4 CPUs @ $0.05 = $0.20
        // 2 GB RAM @ $0.01 = $0.02
        // Total hourly = $0.22
        let profile = BillingProfile::new(0.05, 0.01, 0.0);
        let cost = config.estimate_cost(&profile);

        assert!((cost.hourly_total - 0.22).abs() < f64::EPSILON);
        assert!(0.22f64.mul_add(-730.0, cost.monthly_total).abs() < 1e-10);
    }
}
