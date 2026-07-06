//! Billing estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the resource
//! configuration of a system (`VmConfig`) and its usage (`MetricsSnapshot`)
//! with configurable pricing models to estimate the financial cost of running the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingModel, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024, // 1 GB
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::default(),
//! };
//!
//! let pricing = PricingModel::new(0.05, 0.01); // $0.05 per vCPU/hr, $0.01 per GB/hr
//! let cost = config.estimate_monthly_cost(&pricing);
//! println!("Estimated monthly cost: ${:.2}", cost);
//! ```

use crate::config::VmConfig;
use crate::metrics::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing model for resources.
///
/// # Abstract
/// Holds the unit costs needed to estimate the financial impact of a micro-VM.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::PricingModel;
///
/// let pricing = PricingModel::new(0.04, 0.005);
/// assert_eq!(pricing.cpu_hourly_rate, 0.04);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per virtual CPU per hour.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour.
    pub ram_hourly_rate_per_gb: f64,
    /// Cost per GB of network egress data.
    pub network_egress_rate_per_gb: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel` with base CPU and RAM rates.
    ///
    /// # Abstract
    /// A factory method to instantiate a standard pricing model, defaulting
    /// network egress to $0.09 per GB (standard cloud rate).
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_hourly_rate_per_gb: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_hourly_rate_per_gb,
            network_egress_rate_per_gb: 0.09,
        }
    }
}

/// Trait for objects that can estimate their financial cost.
///
/// # Abstract
/// This trait defines the capability to estimate the cost of a micro-VM
/// based on its resource allocation or dynamic usage.
pub trait CostEstimator {
    /// Estimates the monthly cost (assuming 730 hours/month) based on static configuration.
    fn estimate_monthly_cost(&self, pricing: &PricingModel) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_monthly_cost(&self, pricing: &PricingModel) -> f64 {
        let hours_per_month = 730.0;
        let cpus = f64::from(self.cpus);
        let ram_gb = f64::from(self.ram_mib) / 1024.0;

        let cpu_cost = cpus * pricing.cpu_hourly_rate * hours_per_month;
        let ram_cost = ram_gb * pricing.ram_hourly_rate_per_gb * hours_per_month;

        cpu_cost + ram_cost
    }
}

/// Trait for objects that can estimate dynamic usage costs.
pub trait DynamicCostEstimator {
    /// Estimates the cost of network egress based on current metrics.
    fn estimate_network_cost(&self, pricing: &PricingModel) -> f64;
}

impl DynamicCostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_network_cost(&self, pricing: &PricingModel) -> f64 {
        let total_tx_bytes: u64 = self.networks.iter().map(|n| n.tx_bytes).sum();
        let gb = total_tx_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        gb * pricing.network_egress_rate_per_gb
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::config::GuestAgentMode;
    use crate::metrics::{CpuMetrics, MemoryMetrics, NetMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_static_cost_estimator() {
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
            guest_agent: GuestAgentMode::default(),
        };

        let pricing = PricingModel::new(0.05, 0.01);
        // hourly: (4 * 0.05) + (2 * 0.01) = 0.20 + 0.02 = 0.22
        // monthly: 0.22 * 730 = 160.6
        let cost = config.estimate_monthly_cost(&pricing);
        assert!((cost - 160.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dynamic_cost_estimator() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 0.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            memory: MemoryMetrics {
                total_bytes: 0,
                used_bytes: 0,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![
                NetMetrics {
                    interface: "eth0".to_string(),
                    rx_bytes: 0,
                    tx_bytes: 1073741824,
                    rx_packets: 0,
                    tx_packets: 0,
                    rx_errors: 0,
                    tx_errors: 0,
                }, // 1 GB
                NetMetrics {
                    interface: "eth1".to_string(),
                    rx_bytes: 0,
                    tx_bytes: 536870912,
                    rx_packets: 0,
                    tx_packets: 0,
                    rx_errors: 0,
                    tx_errors: 0,
                }, // 0.5 GB
            ],
            processes: vec![],
        };

        let pricing = PricingModel::new(0.0, 0.0); // Only care about egress which is 0.09
        let cost = snap.estimate_network_cost(&pricing);
        // 1.5 GB * 0.09 = 0.135
        assert!((cost - 0.135).abs() < f64::EPSILON);
    }
}
