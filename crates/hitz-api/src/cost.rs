#![allow(clippy::cast_precision_loss)]
//! FinOps Cost Estimator Module.
//!
//! # Abstract
//! This module provides tools to estimate hourly infrastructure costs
//! based on the VM configuration and quantify "wasted" spend based on
//! real-time resource utilization telemetry.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostFactors, CostEstimator, WasteEstimator, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let factors = CostFactors::new(0.05, 0.01);
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
//! // Calculate fixed hourly cost
//! let hourly_cost = config.estimate_hourly_cost(&factors);
//! assert!(hourly_cost > 0.0);
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0, // Only 10% used, 90% wasted
//!         per_core: vec![10.0; 4],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4096 * 1024 * 1024,
//!         used_bytes: 1024 * 1024 * 1024, // 25% used, 75% wasted
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
//! // Calculate how much money is burning for no reason
//! let hourly_waste = snap.estimate_hourly_waste(&factors, &config);
//! assert!(hourly_waste > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable cost factors for the cloud region or on-prem hardware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostFactors {
    /// Cost per virtual CPU core per hour in dollars.
    pub cpu_cost_per_hour: f64,
    /// Cost per GB of RAM per hour in dollars.
    pub ram_cost_per_gb_hour: f64,
}

impl CostFactors {
    /// Creates a new `CostFactors` profile.
    #[must_use]
    pub const fn new(cpu_cost_per_hour: f64, ram_cost_per_gb_hour: f64) -> Self {
        Self {
            cpu_cost_per_hour,
            ram_cost_per_gb_hour,
        }
    }
}

/// Trait to estimate the fixed infrastructural cost of a configuration.
pub trait CostEstimator {
    /// Estimates the hourly cost in dollars based on allocated resources.
    fn estimate_hourly_cost(&self, factors: &CostFactors) -> f64;
}

impl CostEstimator for VmConfig {
    fn estimate_hourly_cost(&self, factors: &CostFactors) -> f64 {
        let cpu_cost = f64::from(self.cpus) * factors.cpu_cost_per_hour;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * factors.ram_cost_per_gb_hour;
        cpu_cost + ram_cost
    }
}

/// Trait to estimate financial waste based on underutilization.
pub trait WasteEstimator {
    /// Estimates the hourly cost in dollars of unutilized resources.
    fn estimate_hourly_waste(&self, factors: &CostFactors, config: &VmConfig) -> f64;
}

impl WasteEstimator for MetricsSnapshot {
    fn estimate_hourly_waste(&self, factors: &CostFactors, config: &VmConfig) -> f64 {
        let total_hourly_cost = config.estimate_hourly_cost(factors);

        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let cpu_waste_pct = 1.0 - cpu_utilization;

        let ram_bytes = u64::from(config.ram_mib) * 1024 * 1024;
        let mem_waste_pct = if ram_bytes > 0 {
            let mem_pct = (self.memory.used_bytes as f64 / ram_bytes as f64).clamp(0.0, 1.0);
            1.0 - mem_pct
        } else {
            0.0
        };

        let cpu_cost = f64::from(config.cpus) * factors.cpu_cost_per_hour;
        let ram_cost = total_hourly_cost - cpu_cost;

        cpu_cost.mul_add(cpu_waste_pct, ram_cost * mem_waste_pct)
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
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

    fn test_snapshot(cpu_pct: f32, mem_used_mib: u32, mem_total_mib: u32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: u64::from(mem_total_mib) * 1024 * 1024,
                used_bytes: u64::from(mem_used_mib) * 1024 * 1024,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        }
    }

    #[test]
    fn test_cost_estimator() {
        let factors = CostFactors::new(0.10, 0.05); // $0.10 per CPU, $0.05 per GB
        let config = test_config(4, 2048); // 4 CPUs, 2 GB RAM

        let cost = config.estimate_hourly_cost(&factors);
        assert!(
            (cost - 0.50).abs() < f64::EPSILON,
            "Expected 0.50, got {cost}"
        );
    }

    #[test]
    fn test_waste_estimator_full_utilization() {
        let factors = CostFactors::new(0.10, 0.05);
        let config = test_config(4, 2048);
        let snap = test_snapshot(100.0, 2048, 2048);

        let waste = snap.estimate_hourly_waste(&factors, &config);
        assert!(
            (waste - 0.0).abs() < f64::EPSILON,
            "Expected 0 waste, got {waste}"
        );
    }

    #[test]
    fn test_waste_estimator_partial_utilization() {
        let factors = CostFactors::new(0.10, 0.05);
        let config = test_config(4, 2048); // Total cost = $0.50/hr
        let snap = test_snapshot(50.0, 1024, 2048); // 50% CPU, 50% RAM

        let waste = snap.estimate_hourly_waste(&factors, &config);
        // 50% of CPU cost ($0.40) = $0.20 wasted
        // 50% of RAM cost ($0.10) = $0.05 wasted
        // Total waste = $0.25/hr
        assert!(
            (waste - 0.25).abs() < f64::EPSILON,
            "Expected 0.25, got {waste}"
        );
    }
}
