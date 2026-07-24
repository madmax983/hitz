//! GreenOps module for evaluating combined carbon emissions and resource efficiency.
//!
//! # Abstract
//! This module combines the `CarbonEstimator` and `EfficiencyScorer` into a single
//! unified analysis, providing a holistic view of a VM's environmental impact.

use crate::{CarbonEstimator, EfficiencyScore, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The combined result of a `GreenOps` analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GreenOpsResult {
    /// The estimated carbon emissions in mg CO2/sec.
    pub carbon_emissions_mg_per_sec: f64,
    /// The resource efficiency score and insights.
    pub efficiency: EfficiencyScore,
    /// Whether the VM is considered a "Green" workload (high efficiency, low emissions).
    pub is_green: bool,
}

/// Trait for objects that can perform a `GreenOps` analysis.
pub trait GreenOpsAnalyzer {
    /// Performs a combined `GreenOps` analysis.
    fn analyze_greenops(&self, factors: &EmissionFactors, vcpus: u32) -> GreenOpsResult;
}

impl GreenOpsAnalyzer for MetricsSnapshot {
    fn analyze_greenops(&self, factors: &EmissionFactors, vcpus: u32) -> GreenOpsResult {
        let carbon = self.estimate_carbon(factors, vcpus);
        let efficiency = self.calculate_efficiency();

        // Heuristic: A workload is "Green" if it operates highly efficiently (>80 score)
        // or is so idle it barely emits carbon (<5 mg/sec).
        let is_green = efficiency.score > 80.0 || carbon < 5.0;

        GreenOpsResult {
            carbon_emissions_mg_per_sec: carbon,
            efficiency,
            is_green,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu: f32, ram_pct: f64) -> MetricsSnapshot {
        let total_mem = 1024 * 1024 * 1024;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let used_mem = (total_mem as f64 * (ram_pct / 100.0)) as u64;

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
                free_bytes: total_mem - used_mem,
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
    fn test_greenops_highly_efficient() {
        // 95% CPU, 90% RAM. High usage -> highly efficient.
        let snap = dummy_snapshot(95.0, 90.0);
        let factors = EmissionFactors::new(200.0);
        let result = snap.analyze_greenops(&factors, 4);

        assert!(result.efficiency.score > 90.0);
        assert!(result.is_green);
    }

    #[test]
    fn test_greenops_idle_low_emissions() {
        // 0% CPU, 10% RAM. Idle, but overall emissions should be low enough to be "green" by default threshold.
        let snap = dummy_snapshot(0.0, 10.0);
        let factors = EmissionFactors::new(20.0); // Very clean grid
        let result = snap.analyze_greenops(&factors, 1); // 1 vcpu to keep emissions low

        assert!(result.carbon_emissions_mg_per_sec < 5.0);
        assert!(result.efficiency.score < 50.0); // Terribly inefficient
        assert!(result.is_green); // But low absolute emissions, so we consider it OK for now.
    }

    #[test]
    fn test_greenops_wasteful() {
        // 5% CPU, 10% RAM. Highly inefficient, and dirty grid -> not green.
        let snap = dummy_snapshot(5.0, 10.0);
        let factors = EmissionFactors::new(6000.0); // Very dirty grid to ensure emissions > 5.0 mg/s
        let result = snap.analyze_greenops(&factors, 8); // 8 vcpus idling

        assert!(result.carbon_emissions_mg_per_sec > 5.0);
        assert!(result.efficiency.score < 50.0);
        assert!(!result.is_green);
    }
}
