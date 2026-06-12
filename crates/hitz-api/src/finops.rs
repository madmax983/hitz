//! Financial Operations (FinOps) Cloud Economics module.
//!
//! # Abstract
//! Calculates the estimated cost of running a micro-VM based on its provisioned
//! capacity and its actual utilization, exposing potential cost savings (wasted spend).

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Configurable pricing model for estimating costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour in USD.
    pub hourly_cost_per_vcpu: f64,
    /// Cost per GiB of RAM per hour in USD.
    pub hourly_cost_per_gib: f64,
}

impl Default for PricingModel {
    fn default() -> Self {
        Self {
            hourly_cost_per_vcpu: 0.05, // e.g., $0.05 / vCPU / hr
            hourly_cost_per_gib: 0.01,  // e.g., $0.01 / GiB / hr
        }
    }
}

/// The result of a cost estimation for a specific point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostAnalysis {
    /// The theoretical cost per hour based purely on provisioned resources.
    pub provisioned_hourly_cost: f64,
    /// The estimated cost per hour of resources actually utilized.
    pub effective_hourly_cost: f64,
    /// The estimated cost per hour of resources provisioned but sitting idle.
    pub wasted_hourly_cost: f64,
}

/// Trait to estimate `FinOps` costs.
pub trait CostEstimator {
    /// Estimates the current hourly cost run-rate based on utilization and config.
    fn estimate_cost(&self, config: &VmConfig, pricing: &PricingModel) -> CostAnalysis;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_cost(&self, config: &VmConfig, pricing: &PricingModel) -> CostAnalysis {
        let vcpus_f64 = f64::from(config.cpus);

        // RAM is measured in MiB, convert to GiB for pricing
        // 1 GiB = 1024 MiB
        let ram_gib_f64 = f64::from(config.ram_mib) / 1024.0;

        let provisioned_hourly_cost = vcpus_f64.mul_add(
            pricing.hourly_cost_per_vcpu,
            ram_gib_f64 * pricing.hourly_cost_per_gib,
        );

        // Effective CPU usage based on actual percentage.
        // total_pct is 0.0 to 100.0, so divide by 100 to get a 0.0 to 1.0 multiplier.
        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let effective_cpu_cost = vcpus_f64 * cpu_utilization * pricing.hourly_cost_per_vcpu;

        // Effective RAM usage based on actual consumed bytes vs total.
        let ram_utilization = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64 / self.memory.total_bytes as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let effective_ram_cost = ram_gib_f64 * ram_utilization * pricing.hourly_cost_per_gib;

        let effective_hourly_cost = effective_cpu_cost + effective_ram_cost;

        // Wasted cost is strictly whatever we pay for but aren't currently using.
        let wasted_hourly_cost = (provisioned_hourly_cost - effective_hourly_cost).max(0.0);

        CostAnalysis {
            // We round the results slightly to avoid float precision issues in tests.
            provisioned_hourly_cost: (provisioned_hourly_cost * 10_000.0).round() / 10_000.0,
            effective_hourly_cost: (effective_hourly_cost * 10_000.0).round() / 10_000.0,
            wasted_hourly_cost: (wasted_hourly_cost * 10_000.0).round() / 10_000.0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn dummy_config(cpus: u32, ram_mib: u32) -> VmConfig {
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

    fn dummy_snapshot(cpu_pct: f32, ram_used_mib: u64, ram_total_mib: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_total_mib * 1024 * 1024,
                used_bytes: ram_used_mib * 1024 * 1024,
                free_bytes: (ram_total_mib.saturating_sub(ram_used_mib)) * 1024 * 1024,
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
    fn test_perfectly_utilized_vm() {
        let config = dummy_config(4, 4096); // 4 vCPU, 4 GiB
        let snap = dummy_snapshot(100.0, 4096, 4096); // 100% CPU, 100% RAM
        let pricing = PricingModel::default();

        let analysis = snap.estimate_cost(&config, &pricing);

        // Expected provisioned: 4 * 0.05 + 4 * 0.01 = $0.24 / hr
        assert_eq!(analysis.provisioned_hourly_cost, 0.24);
        assert_eq!(analysis.wasted_hourly_cost, 0.0);
        assert_eq!(analysis.effective_hourly_cost, 0.24);
    }

    #[test]
    fn test_completely_idle_vm() {
        let config = dummy_config(2, 2048); // 2 vCPU, 2 GiB
        let snap = dummy_snapshot(0.0, 0, 2048); // 0% CPU, 0% RAM
        let pricing = PricingModel::default();

        let analysis = snap.estimate_cost(&config, &pricing);

        // Expected provisioned: 2 * 0.05 + 2 * 0.01 = $0.12 / hr
        assert_eq!(analysis.provisioned_hourly_cost, 0.12);
        assert_eq!(analysis.wasted_hourly_cost, 0.12);
        assert_eq!(analysis.effective_hourly_cost, 0.0);
    }

    #[test]
    fn test_partially_utilized_vm() {
        let config = dummy_config(4, 4096); // 4 vCPU, 4 GiB
        // 50% CPU, 25% RAM
        let snap = dummy_snapshot(50.0, 1024, 4096);
        let pricing = PricingModel {
            hourly_cost_per_vcpu: 0.10, // $0.10 / vCPU
            hourly_cost_per_gib: 0.02,  // $0.02 / GiB
        };

        let analysis = snap.estimate_cost(&config, &pricing);

        // Provisioned: 4 * 0.10 + 4 * 0.02 = 0.40 + 0.08 = 0.48
        // Effective CPU: 4 * 0.50 * 0.10 = 0.20
        // Effective RAM: (1024 / 1024) * 0.02 = 1.0 * 0.02 = 0.02
        // Effective Total: 0.22
        // Wasted Total: 0.48 - 0.22 = 0.26

        assert_eq!(analysis.provisioned_hourly_cost, 0.48);
        assert_eq!(analysis.effective_hourly_cost, 0.22);
        assert_eq!(analysis.wasted_hourly_cost, 0.26);
    }
}
