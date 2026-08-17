//! `FinOps` Cost Estimation for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that analyzes a `VmConfig`
//! alongside a `PricingModel` to estimate the hourly and monthly infrastructure
//! costs associated with running a specific micro-VM.
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
//! let pricing = PricingModel::new(0.02, 0.005, 0.001);
//! let analysis = config.estimate_cost(&pricing);
//! assert!(analysis.hourly_cost > 0.0);
//! ```

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing model for infrastructure components.
///
/// # Abstract
/// This struct holds the per-hour cost rates for CPU, RAM, and Disk resources,
/// enabling translation from technical specifications to financial impact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour in USD.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_gb_hourly_rate: f64,
    /// Cost per GB of disk storage per hour in USD.
    pub disk_gb_hourly_rate: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel` with the given rates.
    #[must_use]
    pub const fn new(cpu_rate: f64, ram_rate: f64, disk_rate: f64) -> Self {
        Self {
            cpu_hourly_rate: cpu_rate,
            ram_gb_hourly_rate: ram_rate,
            disk_gb_hourly_rate: disk_rate,
        }
    }
}

/// The computed cost analysis for a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostAnalysis {
    /// Total estimated cost per hour in USD.
    pub hourly_cost: f64,
    /// Total estimated cost per month (assuming 730 hours) in USD.
    pub monthly_cost: f64,
    /// Detailed breakdown of the costs.
    pub breakdown: CostBreakdown,
}

/// Detailed breakdown of the estimated costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// CPU cost per hour.
    pub cpu_hourly: f64,
    /// RAM cost per hour.
    pub ram_hourly: f64,
    /// Disk cost per hour.
    pub disk_hourly: f64,
}

/// Trait for objects that can estimate their financial cost.
pub trait CostEstimator {
    /// Estimates the cost based on the provided pricing model.
    fn estimate_cost(&self, pricing: &PricingModel) -> CostAnalysis;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_cost(&self, pricing: &PricingModel) -> CostAnalysis {
        let cpu_cost = f64::from(self.cpus) * pricing.cpu_hourly_rate;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.ram_gb_hourly_rate;

        let disk_gb = if self.disk_path.is_some() { 10.0 } else { 0.0 };
        let disk_cost = disk_gb * pricing.disk_gb_hourly_rate;

        let hourly_cost = cpu_cost + ram_cost + disk_cost;
        let monthly_cost = hourly_cost * 730.0;

        CostAnalysis {
            hourly_cost,
            monthly_cost,
            breakdown: CostBreakdown {
                cpu_hourly: cpu_cost,
                ram_hourly: ram_cost,
                disk_hourly: disk_cost,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    fn default_config() -> VmConfig {
        VmConfig {
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
        }
    }

    #[test]
    fn test_cost_estimator_basic() {
        let config = default_config();
        let pricing = PricingModel::new(0.05, 0.01, 0.005);

        let analysis = config.estimate_cost(&pricing);
        assert!((analysis.hourly_cost - 0.11).abs() < f64::EPSILON);
        assert!((analysis.breakdown.cpu_hourly - 0.10).abs() < f64::EPSILON);
        assert!((analysis.breakdown.ram_hourly - 0.01).abs() < f64::EPSILON);
        assert_eq!(analysis.breakdown.disk_hourly, 0.0);
    }

    #[test]
    fn test_cost_estimator_with_disk() {
        let mut config = default_config();
        config.disk_path = Some(PathBuf::from("disk.img"));

        let pricing = PricingModel::new(0.05, 0.01, 0.005);

        let analysis = config.estimate_cost(&pricing);
        assert!((analysis.hourly_cost - 0.16).abs() < f64::EPSILON);
        assert!((analysis.breakdown.disk_hourly - 0.05).abs() < f64::EPSILON);
    }
}
