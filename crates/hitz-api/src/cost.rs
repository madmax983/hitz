//! Cloud Cost Estimator.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the static configuration
//! (`VmConfig`) and real-time utilization (`MetricsSnapshot`) with configurable cloud
//! pricing models to estimate the running cost of a VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CloudPricing, CostEstimator, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let pricing = CloudPricing {
//!     cpu_hourly_rate: 0.05,
//!     ram_gib_hourly_rate: 0.01,
//! };
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
//! let cost = config.estimate_hourly_cost(&pricing);
//! assert!((cost - 0.22).abs() < f64::EPSILON);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Cloud pricing model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPricing {
    /// Cost per CPU core per hour.
    pub cpu_hourly_rate: f64,
    /// Cost per GiB of RAM per hour.
    pub ram_gib_hourly_rate: f64,
}

impl CloudPricing {
    /// Create a new cloud pricing model.
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_gib_hourly_rate: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_gib_hourly_rate,
        }
    }
}

/// Trait to estimate the running cost of a VM.
pub trait CostEstimator {
    /// Estimate the hourly cost of the VM based on the pricing model.
    fn estimate_hourly_cost(&self, pricing: &CloudPricing) -> f64;

    /// Estimate the monthly cost (assuming 730 hours/month).
    fn estimate_monthly_cost(&self, pricing: &CloudPricing) -> f64 {
        self.estimate_hourly_cost(pricing) * 730.0
    }
}

impl CostEstimator for VmConfig {
    fn estimate_hourly_cost(&self, pricing: &CloudPricing) -> f64 {
        let cpu_cost = f64::from(self.cpus) * pricing.cpu_hourly_rate;
        let ram_gib = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gib * pricing.ram_gib_hourly_rate;
        cpu_cost + ram_cost
    }
}

impl CostEstimator for MetricsSnapshot {
    fn estimate_hourly_cost(&self, pricing: &CloudPricing) -> f64 {
        let active_cpus = f64::from(
            self.cpu
                .per_core
                .iter()
                .map(|&usage| usage / 100.0)
                .sum::<f32>(),
        );
        let cpu_cost = active_cpus * pricing.cpu_hourly_rate;
        #[allow(clippy::cast_precision_loss)]
        let used_ram_gib = self.memory.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = used_ram_gib * pricing.ram_gib_hourly_rate;
        cpu_cost + ram_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_cost() {
        let pricing = CloudPricing::new(0.05, 0.01);
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
            guest_agent: crate::GuestAgentMode::Auto,
        };

        let hourly = config.estimate_hourly_cost(&pricing);
        assert!((hourly - 0.22).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_cost() {
        let pricing = CloudPricing::new(0.05, 0.01);
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![100.0, 0.0, 100.0, 0.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 4 * 1024 * 1024 * 1024,
                used_bytes: 2 * 1024 * 1024 * 1024,
                free_bytes: 2 * 1024 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        // CPUs: 2.0 active * 0.05 = 0.10
        // RAM: 2.0 GiB used * 0.01 = 0.02
        // Total: 0.12
        let hourly = snap.estimate_hourly_cost(&pricing);
        assert!((hourly - 0.12).abs() < f64::EPSILON);
    }
}
