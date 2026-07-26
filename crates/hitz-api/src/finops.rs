//! FinOps cost and waste estimator for micro-VMs.
//!
//! # Abstract
//! This module connects the resource allocation (`VmConfig`) and the real-time utilization
//! (`MetricsSnapshot`) with configurable hourly rates to estimate the real-time monetary cost
//! and calculate wasted spending on unused resources.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FinOpsEstimator, HourlyRates, MetricsSnapshot, CpuMetrics, MemoryMetrics, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 25.0, // Only using 1 of 4 CPUs
//!         per_core: vec![100.0, 0.0, 0.0, 0.0],
//!         load_avg: [1.0, 0.5, 0.2],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4096 * 1024 * 1024,
//!         used_bytes: 1024 * 1024 * 1024, // Only using 1GB
//!         free_bytes: 3072 * 1024 * 1024,
//!         buffers_bytes: 0,
//!         cached_bytes: 0,
//!         swap_total: 0,
//!         swap_used: 0,
//!     },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let rates = HourlyRates { cpu_core_ph: 0.05, ram_gb_ph: 0.01 };
//! let analysis = snap.estimate_cost(&config, &rates);
//!
//! assert!((analysis.total_hourly_cost - 0.24).abs() < f64::EPSILON); // 4 CPUs * 0.05 + 4GB * 0.01
//! assert!(analysis.wasted_hourly_cost > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable hourly rates for `FinOps` calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourlyRates {
    /// Cost per CPU core per hour.
    pub cpu_core_ph: f64,
    /// Cost per GB of RAM per hour.
    pub ram_gb_ph: f64,
}

/// The result of a `FinOps` analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostAnalysis {
    /// The total cost per hour for the allocated resources.
    pub total_hourly_cost: f64,
    /// The cost per hour of resources allocated but not currently utilized.
    pub wasted_hourly_cost: f64,
    /// The cost per hour of resources actually utilized.
    pub utilized_hourly_cost: f64,
}

/// Trait to estimate `FinOps` metrics.
pub trait FinOpsEstimator {
    /// Estimates the current monetary cost and waste for the VM.
    fn estimate_cost(&self, config: &VmConfig, rates: &HourlyRates) -> CostAnalysis;
}

impl FinOpsEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, config: &VmConfig, rates: &HourlyRates) -> CostAnalysis {
        let cpu_allocated_cost = f64::from(config.cpus) * rates.cpu_core_ph;
        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let ram_allocated_cost = ram_gb * rates.ram_gb_ph;

        let total_hourly_cost = cpu_allocated_cost + ram_allocated_cost;

        let cpu_utilization = f64::from(self.cpu.total_pct) / 100.0;
        let cpu_utilized_cost = cpu_allocated_cost * cpu_utilization;

        let memory_utilization = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64) / (self.memory.total_bytes as f64)
        } else {
            0.0
        };
        let ram_utilized_cost = ram_allocated_cost * memory_utilization;

        let utilized_hourly_cost = cpu_utilized_cost + ram_utilized_cost;
        let wasted_hourly_cost = total_hourly_cost - utilized_hourly_cost;

        CostAnalysis {
            total_hourly_cost,
            wasted_hourly_cost,
            utilized_hourly_cost,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, GuestAgentMode};
    use std::path::PathBuf;

    #[test]
    fn test_finops_estimation() {
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

        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![100.0, 100.0, 0.0, 0.0],
                load_avg: [2.0, 1.0, 0.5],
            },
            memory: MemoryMetrics {
                total_bytes: 4096 * 1024 * 1024,
                used_bytes: 2048 * 1024 * 1024,
                free_bytes: 2048 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let rates = HourlyRates { cpu_core_ph: 0.10, ram_gb_ph: 0.05 };
        let analysis = snap.estimate_cost(&config, &rates);

        assert!((analysis.total_hourly_cost - 0.60).abs() < f64::EPSILON); // 4 * 0.10 + 4 * 0.05 = 0.40 + 0.20
        assert!((analysis.utilized_hourly_cost - 0.30).abs() < f64::EPSILON); // 50% CPU = 0.20, 50% RAM = 0.10
        assert!((analysis.wasted_hourly_cost - 0.30).abs() < f64::EPSILON); // Total - Utilized
    }
}
