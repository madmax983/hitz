use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Cloud pricing configuration for `FinOps` estimates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPricing {
    /// Hourly cost per vCPU.
    pub per_vcpu_hourly: f64,
    /// Hourly cost per GB of RAM.
    pub per_gb_ram_hourly: f64,
}

impl CloudPricing {
    /// Creates a new `CloudPricing` instance.
    #[must_use]
    pub const fn new(per_vcpu_hourly: f64, per_gb_ram_hourly: f64) -> Self {
        Self {
            per_vcpu_hourly,
            per_gb_ram_hourly,
        }
    }
}

/// Trait to estimate `FinOps` costs for a resource.
pub trait FinOpsEstimator {
    /// Estimates the monthly cloud cost based on the provided pricing.
    fn estimate_monthly_cost(&self, pricing: &CloudPricing) -> f64;
}

impl FinOpsEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_monthly_cost(&self, pricing: &CloudPricing) -> f64 {
        let hours_per_month = 730.0;
        let cpus_cost = f64::from(self.cpus) * pricing.per_vcpu_hourly * hours_per_month;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.per_gb_ram_hourly * hours_per_month;
        cpus_cost + ram_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VmConfig, GuestAgentMode};
    use std::path::PathBuf;

    #[test]
    #[allow(clippy::float_cmp, clippy::suboptimal_flops)]
    fn test_cloud_cost_estimation() {
        let pricing = CloudPricing::new(0.04, 0.005);
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 4096,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };
        let cost = config.estimate_monthly_cost(&pricing);
        assert!((cost - (4.0 * 0.04 * 730.0 + 4.0 * 0.005 * 730.0)).abs() < f64::EPSILON);
    }
}
