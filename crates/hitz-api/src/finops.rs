//! FinOps module for estimating operational costs.
//!
//! # Abstract
//! This module correlates resource allocations (`VmConfig`) and actual
//! utilization (`MetricsSnapshot`) with defined billing rates to estimate
//! hourly run costs and identify potential savings from underutilized resources.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, FinOpsRates, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // Assume rates of $0.05/vCPU/hr and $0.01/GB RAM/hr
//! let rates = FinOpsRates {
//!     cpu_hourly_rate: 0.05,
//!     ram_gb_hourly_rate: 0.01,
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
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0, // Underutilized!
//!         per_core: vec![10.0; 4],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 2048 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 1536 * 1024 * 1024,
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
//! let analysis = snap.analyze_costs(&config, &rates);
//! assert!(analysis.wasted_hourly_cost > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Billing rates for resource consumption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsRates {
    /// Cost per vCPU per hour.
    pub cpu_hourly_rate: f64,
    /// Cost per GiB of RAM per hour.
    pub ram_gb_hourly_rate: f64,
}

/// The result of a `FinOps` analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsAnalysis {
    /// The total estimated cost to run this VM for one hour.
    pub total_hourly_cost: f64,
    /// The estimated cost per hour that is "wasted" due to underutilization.
    pub wasted_hourly_cost: f64,
    /// Actionable insights for cost savings.
    pub insights: Vec<String>,
}

/// Trait to analyze financial operational costs.
pub trait FinOpsAnalyzer {
    /// Analyzes metrics and configuration to estimate costs and waste.
    fn analyze_costs(&self, config: &VmConfig, rates: &FinOpsRates) -> FinOpsAnalysis;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn analyze_costs(&self, config: &VmConfig, rates: &FinOpsRates) -> FinOpsAnalysis {
        let cpu_cost = f64::from(config.cpus) * rates.cpu_hourly_rate;

        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let ram_cost = ram_gb * rates.ram_gb_hourly_rate;

        let total_hourly_cost = cpu_cost + ram_cost;

        // Calculate waste based on unutilized capacity
        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let cpu_wasted_cost = cpu_cost * (1.0 - cpu_utilization);

        let ram_total_bytes = f64::from(config.ram_mib) * 1024.0 * 1024.0;
        let ram_wasted_cost = if ram_total_bytes > 0.0 {
            let mem_utilization = (self.memory.used_bytes as f64 / ram_total_bytes).clamp(0.0, 1.0);
            ram_cost * (1.0 - mem_utilization)
        } else {
            0.0
        };

        let wasted_hourly_cost = cpu_wasted_cost + ram_wasted_cost;

        let mut insights = Vec::new();
        if cpu_wasted_cost > 0.1 * cpu_cost {
            insights.push(format!(
                "CPU underutilized: wasting ${cpu_wasted_cost:.2}/hr"
            ));
        }
        if ram_wasted_cost > 0.1 * ram_cost {
            insights.push(format!(
                "RAM underutilized: wasting ${ram_wasted_cost:.2}/hr"
            ));
        }

        FinOpsAnalysis {
            total_hourly_cost,
            wasted_hourly_cost,
            insights,
        }
    }
}

#[cfg(test)]
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

    fn test_snapshot(cpu_pct: f32, ram_used_mib: u32, ram_total_mib: u32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: u64::from(ram_total_mib) * 1024 * 1024,
                used_bytes: u64::from(ram_used_mib) * 1024 * 1024,
                free_bytes: u64::from(ram_total_mib.saturating_sub(ram_used_mib)) * 1024 * 1024,
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
    fn test_finops_analysis_underutilized() {
        // Rates: $0.10 per CPU/hr, $0.05 per GB RAM/hr
        let rates = FinOpsRates {
            cpu_hourly_rate: 0.10,
            ram_gb_hourly_rate: 0.05,
        };

        // VM Config: 4 CPUs, 4 GB RAM = 4096 MiB
        let config = test_config(4, 4096);

        // Expected total cost: (4 * 0.10) + (4 * 0.05) = $0.40 + $0.20 = $0.60 / hr

        // Snapshot: 10% CPU usage, 1024 MiB RAM usage (25% usage)
        let snap = test_snapshot(10.0, 1024, 4096);

        let analysis = snap.analyze_costs(&config, &rates);

        // Total cost should be roughly 0.60
        assert!((analysis.total_hourly_cost - 0.60).abs() < 0.001);

        // CPU wasted: 90% of $0.40 = $0.36
        // RAM wasted: 75% of $0.20 = $0.15
        // Total wasted: $0.51
        assert!((analysis.wasted_hourly_cost - 0.51).abs() < 0.001);

        assert!(!analysis.insights.is_empty());
    }

    #[test]
    fn test_finops_analysis_highly_utilized() {
        let rates = FinOpsRates {
            cpu_hourly_rate: 0.10,
            ram_gb_hourly_rate: 0.05,
        };

        let config = test_config(2, 2048);

        // Snapshot: 95% CPU, 90% RAM
        let snap = test_snapshot(95.0, 1843, 2048);

        let analysis = snap.analyze_costs(&config, &rates);

        assert!((analysis.total_hourly_cost - 0.30).abs() < 0.001); // (2*0.1) + (2*0.05)

        // Waste should be very small
        assert!(analysis.wasted_hourly_cost < 0.05);
    }
}
