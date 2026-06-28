//! Billing and Cost Estimation Module
//!
//! # Abstract
//! This module provides a way to calculate the cost of running a micro-VM,
//! based on static allocations (`VmConfig`) and dynamic usage (`MetricsDiff`).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, BillingRates, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let rates = BillingRates::new(0.05, 0.01);
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
//! let hourly_cost = config.estimate_cost(&rates);
//! assert!(hourly_cost > 0.0);
//! ```

#[cfg(feature = "diff")]
use crate::MetricsDiff;
use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Billing rates for VM resources in arbitrary currency units per hour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingRates {
    /// Cost per vCPU per hour.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour.
    pub ram_gb_hourly_rate: f64,
    /// Cost per GB of network egress.
    pub net_egress_gb_rate: f64,
    /// Cost per GB of disk writes.
    pub disk_write_gb_rate: f64,
}

impl BillingRates {
    /// Creates a new `BillingRates` profile with simple hourly rates for CPU and RAM.
    /// Sets default networking and disk rates to zero.
    #[must_use]
    pub const fn new(cpu_rate: f64, ram_rate: f64) -> Self {
        Self {
            cpu_hourly_rate: cpu_rate,
            ram_gb_hourly_rate: ram_rate,
            net_egress_gb_rate: 0.0,
            disk_write_gb_rate: 0.0,
        }
    }
}

/// Trait to estimate the billing cost.
pub trait CostEstimator {
    /// Calculates the estimated cost per hour based on current state or allocations.
    fn estimate_cost(&self, rates: &BillingRates) -> f64;
}

impl CostEstimator for VmConfig {
    #[allow(clippy::suboptimal_flops)]
    fn estimate_cost(&self, rates: &BillingRates) -> f64 {
        let cpu_cost = f64::from(self.cpus) * rates.cpu_hourly_rate;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * rates.ram_gb_hourly_rate;
        cpu_cost + ram_cost
    }
}

#[cfg(feature = "diff")]
impl CostEstimator for MetricsDiff {
    #[allow(clippy::suboptimal_flops)]
    fn estimate_cost(&self, rates: &BillingRates) -> f64 {
        let mut hourly_net_cost = 0.0;
        let mut hourly_disk_cost = 0.0;

        for net in &self.networks {
            // tx_bytes_per_sec to GB per hour
            let gb_per_sec = net.tx_bytes_per_sec / (1024.0 * 1024.0 * 1024.0);
            let gb_per_hour = gb_per_sec * 3600.0;
            hourly_net_cost += gb_per_hour * rates.net_egress_gb_rate;
        }

        for disk in &self.disks {
            // write_bytes_per_sec to GB per hour
            let gb_per_sec = disk.write_bytes_per_sec / (1024.0 * 1024.0 * 1024.0);
            let gb_per_hour = gb_per_sec * 3600.0;
            hourly_disk_cost += gb_per_hour * rates.disk_write_gb_rate;
        }

        hourly_net_cost + hourly_disk_cost
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    #[cfg(feature = "diff")]
    use crate::{DiskRate, NetRate};
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_cost() {
        let rates = BillingRates::new(0.05, 0.02);
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

        // 4 * 0.05 = 0.20
        // 2 * 0.02 = 0.04
        // Total = 0.24
        let cost = config.estimate_cost(&rates);
        assert!((cost - 0.24).abs() < f64::EPSILON);
    }

    #[cfg(feature = "diff")]
    #[test]
    fn test_metricsdiff_cost() {
        let mut rates = BillingRates::new(0.05, 0.02);
        rates.net_egress_gb_rate = 0.01;
        rates.disk_write_gb_rate = 0.05;

        // 1 GB per second = 3600 GB per hour
        let diff = MetricsDiff {
            elapsed_secs: 1.0,
            disks: vec![DiskRate {
                name: "vda".to_string(),
                reads_per_sec: 0.0,
                writes_per_sec: 10.0,
                read_bytes_per_sec: 0.0,
                write_bytes_per_sec: 1024.0 * 1024.0 * 1024.0, // 1 GB/s
            }],
            networks: vec![NetRate {
                interface: "eth0".to_string(),
                rx_bytes_per_sec: 0.0,
                tx_bytes_per_sec: 2.0 * 1024.0 * 1024.0 * 1024.0, // 2 GB/s
                rx_packets_per_sec: 0.0,
                tx_packets_per_sec: 100.0,
            }],
        };

        // Network cost: 2 GB/s * 3600 = 7200 GB/hr. 7200 * 0.01 = 72.0
        // Disk cost: 1 GB/s * 3600 = 3600 GB/hr. 3600 * 0.05 = 180.0
        // Total: 252.0
        let cost = diff.estimate_cost(&rates);
        assert!((cost - 252.0).abs() < f64::EPSILON);
    }
}
