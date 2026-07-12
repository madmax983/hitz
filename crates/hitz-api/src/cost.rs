//! FinOps Cost Estimator.
//!
//! # Abstract
//! This module calculates the financial cost of running a VM based on its
//! configuration (`VmConfig`) and current utilization (`MetricsSnapshot`),
//! allowing users to quantify wasted resources.

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Financial rates for computing cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostRates {
    /// Cost per vCPU per hour in USD.
    pub cpu_hourly_rate: f64,
    /// Cost per GB of RAM per hour in USD.
    pub ram_hourly_rate: f64,
}

impl CostRates {
    /// Creates a new `CostRates` with standard cloud-like defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cpu_hourly_rate: 0.05, // $0.05 per vCPU hour
            ram_hourly_rate: 0.01, // $0.01 per GB hour
        }
    }
}

impl Default for CostRates {
    fn default() -> Self {
        Self::new()
    }
}

/// The estimated cost of a workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    /// The total hourly run rate in USD.
    pub hourly_run_rate: f64,
    /// The estimated monthly cost in USD.
    pub monthly_projected: f64,
    /// The financial waste per hour based on underutilization.
    pub wasted_hourly: f64,
}

/// Trait to estimate workload costs.
pub trait CostAnalyzer {
    /// Estimates the current financial cost based on rates.
    fn estimate_cost(&self, config: &VmConfig, rates: &CostRates) -> CostEstimate;
}

impl CostAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, config: &VmConfig, rates: &CostRates) -> CostEstimate {
        let cpu_cost = f64::from(config.cpus) * rates.cpu_hourly_rate;
        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let ram_cost = ram_gb * rates.ram_hourly_rate;

        let hourly_run_rate = cpu_cost + ram_cost;
        let monthly_projected = hourly_run_rate * 24.0 * 30.0;

        // Calculate waste based on utilization
        let cpu_util = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;

        let mem_util = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64 / self.memory.total_bytes as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Waste is the portion of the allocated resources not being actively used
        let cpu_waste = cpu_cost * (1.0 - cpu_util);
        let mem_waste = ram_cost * (1.0 - mem_util);

        let wasted_hourly = cpu_waste + mem_waste;

        CostEstimate {
            hourly_run_rate,
            monthly_projected,
            wasted_hourly,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
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

    fn dummy_snapshot(cpu: f32, ram_pct: f64) -> MetricsSnapshot {
        let total_mem: u64 = 1024 * 1024 * 1024;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        #[allow(clippy::cast_precision_loss)]
        let used_mem: u64 = (total_mem as f64 * ram_pct) as u64;

        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: total_mem,
                used_bytes: used_mem,
                free_bytes: total_mem.saturating_sub(used_mem),
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
    fn test_cost_100_percent_utilization() {
        let config = test_config(4, 4096); // 4 CPUs, 4GB RAM
        let snap = dummy_snapshot(100.0, 1.0); // 100% utilized
        let rates = CostRates::new();

        let cost = snap.estimate_cost(&config, &rates);

        // 4 * 0.05 + 4 * 0.01 = 0.20 + 0.04 = 0.24
        assert!((cost.hourly_run_rate - 0.24).abs() < f64::EPSILON);
        assert!((cost.wasted_hourly - 0.0).abs() < f64::EPSILON); // No waste
    }

    #[test]
    fn test_cost_0_percent_utilization() {
        let config = test_config(4, 4096); // 4 CPUs, 4GB RAM
        let snap = dummy_snapshot(0.0, 0.0); // 0% utilized
        let rates = CostRates::new();

        let cost = snap.estimate_cost(&config, &rates);

        assert!((cost.hourly_run_rate - 0.24).abs() < f64::EPSILON);
        assert!((cost.wasted_hourly - 0.24).abs() < f64::EPSILON); // All waste
    }

    #[test]
    fn test_cost_50_percent_utilization() {
        let config = test_config(4, 4096); // 4 CPUs, 4GB RAM
        let snap = dummy_snapshot(50.0, 0.5); // 50% utilized
        let rates = CostRates::new();

        let cost = snap.estimate_cost(&config, &rates);

        assert!((cost.hourly_run_rate - 0.24).abs() < f64::EPSILON);
        assert!((cost.wasted_hourly - 0.12).abs() < f64::EPSILON); // Half waste
    }
}
