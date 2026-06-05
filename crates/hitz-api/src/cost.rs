//! Cloud Cost Estimator.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the VM's static configuration
//! (`VmConfig`) and its dynamic network telemetry (`MetricsDiff`) to estimate the
//! real-time hourly cost of running the micro-VM, including compute, memory, and data egress.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, CloudPricing, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // 1. Define a VM configuration
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 2048,
//!     cpus: 2,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! // 2. Define the pricing model
//! let mut pricing = CloudPricing::new(0.02, 0.01);
//! pricing.egress_per_gb = 0.09; // $0.09 per GB
//!
//! // 3. Estimate static infrastructure cost
//! let hourly_infra_cost = config.estimate_cost(&pricing);
//! assert_eq!(hourly_infra_cost, 0.06); // 2 * 0.02 + 2 * 0.01
//! ```

#[cfg(feature = "diff")]
use crate::MetricsDiff;
use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Configurable pricing factors for a cloud provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPricing {
    /// Cost per vCPU core per hour (in USD).
    pub vcpu_per_hour: f64,
    /// Cost per GB of RAM per hour (in USD).
    pub ram_gb_per_hour: f64,
    /// Base cost for the VM instance per hour (in USD), ignoring resources.
    pub base_cost_per_hour: f64,
    /// Cost per GB of network egress data (in USD).
    pub egress_per_gb: f64,
}

impl CloudPricing {
    /// Creates a new `CloudPricing` profile with compute and memory costs.
    #[must_use]
    pub const fn new(vcpu_per_hour: f64, ram_gb_per_hour: f64) -> Self {
        Self {
            vcpu_per_hour,
            ram_gb_per_hour,
            base_cost_per_hour: 0.0,
            egress_per_gb: 0.0,
        }
    }
}

/// Trait for objects that can estimate their financial cost.
pub trait CostEstimator {
    /// Estimates the projected hourly cost in USD based on the provided pricing model.
    fn estimate_cost(&self, pricing: &CloudPricing) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, pricing: &CloudPricing) -> f64 {
        let cpu_cost = f64::from(self.cpus) * pricing.vcpu_per_hour;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * pricing.ram_gb_per_hour;

        pricing.base_cost_per_hour + cpu_cost + ram_cost
    }
}

#[cfg(feature = "diff")]
impl CostEstimator for MetricsDiff {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, pricing: &CloudPricing) -> f64 {
        // Calculate total egress bytes per second across all networks
        let total_tx_bytes_per_sec: f64 =
            self.networks.iter().map(|net| net.tx_bytes_per_sec).sum();

        // Convert bytes/sec to GB/hour
        // 1 hour = 3600 seconds
        // 1 GB = 1024^3 bytes
        let tx_gb_per_hour = (total_tx_bytes_per_sec * 3600.0) / (1024.0 * 1024.0 * 1024.0);

        tx_gb_per_hour * pricing.egress_per_gb
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod tests {
    use super::*;
    #[cfg(feature = "diff")]
    use crate::NetRate;
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_cost() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 4096, // 4 GB
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: crate::GuestAgentMode::Auto,
        };

        let pricing = CloudPricing {
            vcpu_per_hour: 0.05,
            ram_gb_per_hour: 0.02,
            base_cost_per_hour: 0.10,
            egress_per_gb: 0.0,
        };

        // CPU: 4 * 0.05 = 0.20
        // RAM: 4 * 0.02 = 0.08
        // Base: 0.10
        // Total: 0.38
        let cost = config.estimate_cost(&pricing);
        assert!((cost - 0.38).abs() < f64::EPSILON);
    }

    #[cfg(feature = "diff")]
    #[test]
    fn test_metrics_diff_egress_cost() {
        let diff = MetricsDiff {
            elapsed_secs: 1.0,
            disks: vec![],
            networks: vec![
                NetRate {
                    interface: "eth0".to_string(),
                    rx_bytes_per_sec: 1024.0,
                    tx_bytes_per_sec: 1_048_576.0, // 1 MB/s
                    rx_packets_per_sec: 10.0,
                    tx_packets_per_sec: 10.0,
                },
                NetRate {
                    interface: "eth1".to_string(),
                    rx_bytes_per_sec: 0.0,
                    tx_bytes_per_sec: 1_048_576.0, // 1 MB/s
                    rx_packets_per_sec: 0.0,
                    tx_packets_per_sec: 10.0,
                },
            ],
        };

        let pricing = CloudPricing {
            vcpu_per_hour: 0.0,
            ram_gb_per_hour: 0.0,
            base_cost_per_hour: 0.0,
            egress_per_gb: 0.10, // $0.10 per GB
        };

        // Total tx: 2 MB/s
        // In an hour: 2 * 3600 = 7200 MB = 7200 / 1024 GB ≈ 7.03125 GB
        // Cost: 7.03125 * 0.10 = 0.703125
        let cost = diff.estimate_cost(&pricing);
        assert!(
            (cost - 0.703_125).abs() < 1e-6,
            "Expected 0.703125, got {cost}"
        );
    }
}
