//! `FinOps` cost projection module.
//!
//! # Abstract
//! Converts `VmConfig` (provisioned capacity) and `MetricsSnapshot` (utilized capacity)
//! into real-time cloud cost projections and waste analytics.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, PricingModel, MetricsSnapshot, VmConfig, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let cfg = VmConfig { cpus: 4, ram_mib: 4096, kernel_path: PathBuf::new(), initramfs_path: None, disk_path: None, cmdline: None, net: None, ports: vec![], guest_cid: 3, guest_agent: GuestAgentMode::Disabled };
//! let snap = MetricsSnapshot { timestamp_ms: 1000, cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0, 10.0, 10.0, 10.0], load_avg: [0.1, 0.1, 0.1] }, memory: MemoryMetrics { total_bytes: 4_294_967_296, used_bytes: 536_870_912, free_bytes: 0, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 }, disks: vec![], networks: vec![], processes: vec![] };
//! let pricing = PricingModel { vcpu_hourly_rate: 0.04048, gb_ram_hourly_rate: 0.004445 };
//! let report = snap.analyze_cost(&cfg, &pricing);
//! assert!(report.waste_cost_per_hour > 0.1);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Pricing model containing hourly rates for cloud resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Hourly rate per vCPU.
    pub vcpu_hourly_rate: f64,
    /// Hourly rate per GB of RAM.
    pub gb_ram_hourly_rate: f64,
}

/// A real-time cloud cost projection and waste analytics report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsReport {
    /// The total cost of provisioned resources per hour.
    pub total_cost_per_hour: f64,
    /// The cost of actually utilized resources per hour.
    pub utilized_cost_per_hour: f64,
    /// The cost of wasted (unutilized) resources per hour.
    pub waste_cost_per_hour: f64,
    /// The percentage of total cost that is wasted.
    pub waste_percentage: f64,
}

/// Trait to analyze financial operations (`FinOps`) costs.
pub trait FinOpsAnalyzer {
    /// Analyzes the cost of a VM given its configuration and a pricing model.
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsReport;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn analyze_cost(&self, config: &VmConfig, pricing: &PricingModel) -> FinOpsReport {
        let vcpus = f64::from(config.cpus);
        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let cpu_cost = vcpus * pricing.vcpu_hourly_rate;
        let ram_cost = ram_gb * pricing.gb_ram_hourly_rate;
        let total_cost = cpu_cost + ram_cost;

        let cpu_util = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let ram_util = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64) / (self.memory.total_bytes as f64)
        } else {
            0.0
        };

        let utilized_cost = (cpu_cost * cpu_util) + (ram_cost * ram_util);
        let waste_cost = total_cost - utilized_cost;
        let waste_percentage = if total_cost > 0.0 {
            (waste_cost / total_cost) * 100.0
        } else {
            0.0
        };

        FinOpsReport {
            total_cost_per_hour: total_cost,
            utilized_cost_per_hour: utilized_cost,
            waste_cost_per_hour: waste_cost,
            waste_percentage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn dummy_config(cpus: u32, ram_mib: u32) -> VmConfig {
        VmConfig {
            cpus,
            ram_mib,
            kernel_path: PathBuf::new(),
            initramfs_path: None,
            disk_path: None,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Disabled,
        }
    }

    fn dummy_snap(cpu_pct: f32, total_bytes: u64, used_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes,
                used_bytes,
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
    fn test_perfect_efficiency() {
        let cfg = dummy_config(4, 4096);
        let snap = dummy_snap(100.0, 4_294_967_296, 4_294_967_296);
        let pricing = PricingModel {
            vcpu_hourly_rate: 1.0,
            gb_ram_hourly_rate: 1.0,
        };
        let report = snap.analyze_cost(&cfg, &pricing);
        assert!((report.total_cost_per_hour - 8.0).abs() < f64::EPSILON);
        assert!((report.utilized_cost_per_hour - 8.0).abs() < f64::EPSILON);
        assert!((report.waste_cost_per_hour - 0.0).abs() < f64::EPSILON);
        assert!((report.waste_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_total_waste() {
        let cfg = dummy_config(2, 2048);
        let snap = dummy_snap(0.0, 2_147_483_648, 0);
        let pricing = PricingModel {
            vcpu_hourly_rate: 2.0,
            gb_ram_hourly_rate: 0.5,
        };
        let report = snap.analyze_cost(&cfg, &pricing);
        assert!((report.total_cost_per_hour - 5.0).abs() < f64::EPSILON);
        assert!((report.utilized_cost_per_hour - 0.0).abs() < f64::EPSILON);
        assert!((report.waste_cost_per_hour - 5.0).abs() < f64::EPSILON);
        assert!((report.waste_percentage - 100.0).abs() < f64::EPSILON);
    }
}
