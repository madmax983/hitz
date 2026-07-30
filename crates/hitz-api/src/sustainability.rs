use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// A unified eco-score evaluating the sustainability of the VM workload.
pub struct SustainabilityScore {
    /// The computed sustainability score from 0.0 to 100.0.
    pub eco_score: f64,
    /// Indicates whether the workload meets the threshold to be considered "green".
    pub is_green: bool,
}

/// Trait to evaluate unified sustainability score.
pub trait SustainabilityScorer {
    /// Calculates the sustainability score based on carbon emission factors and efficiency.
    fn calculate_sustainability(
        &self,
        factors: &EmissionFactors,
        vcpus: u32,
    ) -> SustainabilityScore;
}

impl SustainabilityScorer for MetricsSnapshot {
    fn calculate_sustainability(
        &self,
        factors: &EmissionFactors,
        vcpus: u32,
    ) -> SustainabilityScore {
        let carbon_rate = self.estimate_carbon(factors, vcpus);
        let efficiency = self.calculate_efficiency();

        // Normalize carbon rate (heuristic: max expected rate ~ 20.0 mg/s for normal VM)
        // and combine with efficiency score (0-100).
        let carbon_penalty = (carbon_rate * 5.0).min(50.0);
        let eco_score = (efficiency.score - carbon_penalty).clamp(0.0, 100.0);

        SustainabilityScore {
            eco_score,
            is_green: eco_score > 70.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_sustainability_score() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0],
                load_avg: [0.5, 0.5, 0.5],
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
        let factors = EmissionFactors::new(200.0);
        let score = snap.calculate_sustainability(&factors, 1);
        assert!(score.eco_score > 0.0);
    }
}
