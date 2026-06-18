//! FinOps cost and savings estimator.
//!
//! # Abstract
//! This module connects the `VmConfig` and `rightsizer` recommendations with
//! configurable cloud provider pricing to estimate hourly runtime costs and
//! project potential cost savings (or increases) from scaling operations.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{PricingFactors, CostEstimator, SavingsEstimator, VmConfig, GuestAgentMode, ResizeRecommendation};
//! use std::path::PathBuf;
//!
//! // 1. Define our cloud provider's pricing (e.g., $0.05/vCPU/hr, $0.01/GB RAM/hr)
//! let pricing = PricingFactors::new(0.05, 0.01);
//!
//! // 2. Our current heavy VM config
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 8192, // 8 GB
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! // 3. Estimate our current burn rate
//! let hourly_cost = config.estimate_hourly_cost(&pricing);
//! assert!(hourly_cost > 0.0);
//!
//! // 4. The rightsizer told us to scale down CPU!
//! let recommendations = vec![
//!     ResizeRecommendation::ScaleDownCpu {
//!         current: 4,
//!         suggested: 2,
//!         reason: "Low utilization".to_string(),
//!     }
//! ];
//!
//! // 5. See how much money we save!
//! let savings = recommendations.estimate_monthly_savings(&pricing);
//! println!("We can save ${:.2} a month by scaling down!", savings);
//! ```

use crate::{ResizeRecommendation, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for `FinOps` cost estimations.
///
/// # Abstract
/// This struct holds the assumptions needed to translate a `VmConfig` into
/// a monetary cost per hour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingFactors {
    /// Cost per virtual CPU core per hour (in dollars).
    pub hourly_cost_per_vcpu: f64,
    /// Cost per Gigabyte of RAM per hour (in dollars).
    pub hourly_cost_per_gb_ram: f64,
}

impl PricingFactors {
    /// Creates a new `PricingFactors` profile.
    #[must_use]
    pub const fn new(hourly_cost_per_vcpu: f64, hourly_cost_per_gb_ram: f64) -> Self {
        Self {
            hourly_cost_per_vcpu,
            hourly_cost_per_gb_ram,
        }
    }
}

/// Trait for objects that can estimate their running cost.
pub trait CostEstimator {
    /// Estimates the current operational cost per hour.
    fn estimate_hourly_cost(&self, pricing: &PricingFactors) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_hourly_cost(&self, pricing: &PricingFactors) -> f64 {
        let cpu_cost = f64::from(self.cpus) * pricing.hourly_cost_per_vcpu;

        let gb_ram = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = gb_ram * pricing.hourly_cost_per_gb_ram;

        cpu_cost + ram_cost
    }
}

/// Trait for estimating projected savings (or added costs) based on resize recommendations.
pub trait SavingsEstimator {
    /// Estimates the net savings (positive) or additional costs (negative) per hour
    /// if the given resize recommendations were applied.
    fn estimate_hourly_savings(&self, pricing: &PricingFactors) -> f64;

    /// Estimates the net savings per month (assuming 730 hours/month).
    fn estimate_monthly_savings(&self, pricing: &PricingFactors) -> f64 {
        self.estimate_hourly_savings(pricing) * 730.0
    }
}

impl SavingsEstimator for [ResizeRecommendation] {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_hourly_savings(&self, pricing: &PricingFactors) -> f64 {
        let mut total_savings = 0.0;

        for rec in self {
            match rec {
                ResizeRecommendation::ScaleDownCpu {
                    current, suggested, ..
                } => {
                    let diff = current.saturating_sub(*suggested);
                    total_savings += f64::from(diff) * pricing.hourly_cost_per_vcpu;
                }
                ResizeRecommendation::ScaleUpCpu {
                    current, suggested, ..
                } => {
                    let diff = suggested.saturating_sub(*current);
                    total_savings -= f64::from(diff) * pricing.hourly_cost_per_vcpu;
                }
                ResizeRecommendation::ScaleDownRam {
                    current_mib,
                    suggested_mib,
                    ..
                } => {
                    let diff_mib = current_mib.saturating_sub(*suggested_mib);
                    let diff_gb = f64::from(diff_mib) / 1024.0;
                    total_savings += diff_gb * pricing.hourly_cost_per_gb_ram;
                }
                ResizeRecommendation::ScaleUpRam {
                    current_mib,
                    suggested_mib,
                    ..
                } => {
                    let diff_mib = suggested_mib.saturating_sub(*current_mib);
                    let diff_gb = f64::from(diff_mib) / 1024.0;
                    total_savings -= diff_gb * pricing.hourly_cost_per_gb_ram;
                }
            }
        }

        total_savings
    }
}

#[cfg(test)]
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
    fn test_cost_estimator() {
        let pricing = PricingFactors::new(0.05, 0.01);
        let config = test_config(4, 4096); // 4 vCPUs, 4GB RAM

        // 4 * 0.05 = 0.20
        // 4 * 0.01 = 0.04
        // Total = 0.24 / hr

        let cost = config.estimate_hourly_cost(&pricing);
        assert!((cost - 0.24).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::suboptimal_flops)]
    fn test_savings_estimator_down() {
        let pricing = PricingFactors::new(0.05, 0.01);

        let recs = [
            ResizeRecommendation::ScaleDownCpu {
                current: 4,
                suggested: 2,
                reason: String::new(),
            },
            ResizeRecommendation::ScaleDownRam {
                current_mib: 8192,
                suggested_mib: 4096,
                reason: String::new(),
            },
        ];

        // Save 2 vCPUs: 2 * 0.05 = 0.10
        // Save 4GB RAM: 4 * 0.01 = 0.04
        // Total savings = 0.14 / hr

        let savings = recs.estimate_hourly_savings(&pricing);
        assert!((savings - 0.14).abs() < f64::EPSILON);

        let monthly = recs.estimate_monthly_savings(&pricing);
        assert!((monthly - (0.14 * 730.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_savings_estimator_up() {
        let pricing = PricingFactors::new(0.05, 0.01);

        let recs = [ResizeRecommendation::ScaleUpCpu {
            current: 2,
            suggested: 4,
            reason: String::new(),
        }];

        // Cost an extra 2 vCPUs: 2 * 0.05 = -0.10

        let savings = recs.estimate_hourly_savings(&pricing);
        assert!((savings - (-0.10)).abs() < f64::EPSILON);
    }
}
