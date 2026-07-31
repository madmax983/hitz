//! Sustainability mashup module.
//!
//! # Abstract
//! Combines `CarbonEstimator` and `EfficiencyScorer` to provide a unified `SustainabilityReport`.

use crate::{CarbonEstimator, EfficiencyScore, EfficiencyScorer, EmissionFactors};
use serde::{Deserialize, Serialize};

/// A combined report of carbon emissions and resource efficiency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SustainabilityReport {
    /// The estimated carbon emission rate in mg CO2/sec.
    pub carbon_mg_per_sec: f64,
    /// The computed efficiency score and insights.
    pub efficiency: EfficiencyScore,
}

/// Trait to analyze sustainability metrics.
pub trait SustainabilityAnalyzer {
    /// Generates a sustainability report for the given resource utilization.
    fn analyze_sustainability(&self, factors: &EmissionFactors, vcpus: u32)
    -> SustainabilityReport;
}

impl<T: CarbonEstimator + EfficiencyScorer> SustainabilityAnalyzer for T {
    fn analyze_sustainability(
        &self,
        factors: &EmissionFactors,
        vcpus: u32,
    ) -> SustainabilityReport {
        SustainabilityReport {
            carbon_mg_per_sec: self.estimate_carbon(factors, vcpus),
            efficiency: self.calculate_efficiency(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, MetricsSnapshot};

    #[test]
    fn test_sustainability_analyzer() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 100.0,
                per_core: vec![100.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
                free_bytes: 512 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let factors = EmissionFactors::new(400.0);
        let report = snap.analyze_sustainability(&factors, 1);

        // At 1 vcpu and 100% CPU usage, and 1GB RAM, carbon rate should be 1.75 mg/s
        assert!((report.carbon_mg_per_sec - 1.75).abs() < f64::EPSILON);
    }
}
