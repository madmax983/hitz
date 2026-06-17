//! EcoScore Sustainability Profiling Module.
//!
//! # Abstract
//! This module combines the resource efficiency and carbon footprint estimators
//! to produce an overall `EcoScore`. This enables orchestrators to rank VMs
//! by sustainability, identifying workloads that are both underutilized and
//! carbon-intensive.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoScorer, EcoScoreResult, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Create an idle VM snapshot
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 0.0,
//!         per_core: vec![0.0, 0.0],
//!         load_avg: [0.0, 0.0, 0.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 10 * 1024 * 1024,
//!         free_bytes: 0,
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
//! let factors = EmissionFactors::new(400.0);
//! let eco = snap.calculate_eco_score(&factors, 2);
//!
//! // An idle VM wasting resources has a low eco score
//! assert!(eco.overall_score < 50.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The computed sustainability score for a VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoScoreResult {
    /// Overall sustainability score from 0.0 (terrible) to 100.0 (optimal).
    pub overall_score: f64,
    /// The efficiency component score (0.0 to 100.0).
    pub efficiency_score: f64,
    /// The estimated current carbon emission rate in mg CO2/sec.
    pub emission_rate_mg_sec: f64,
    /// Human-readable insights into sustainability.
    pub insights: Vec<String>,
}

/// Trait to calculate the comprehensive eco score.
pub trait EcoScorer {
    /// Evaluates current metrics and emission factors to produce an `EcoScoreResult`.
    fn calculate_eco_score(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScoreResult;
}

impl EcoScorer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_eco_score(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScoreResult {
        // 1. Get the efficiency baseline
        let efficiency = self.calculate_efficiency();

        // 2. Get the carbon footprint
        let emissions = self.estimate_carbon(factors, vcpus);

        // 3. Compute overall score
        // A perfect score means high efficiency AND low relative carbon footprint.
        // If a VM is perfectly efficient (100.0) we just keep it.
        // But if it's inefficient (e.g. 20.0 score) AND has high emissions because
        // it's wasting lots of idle vCPUs/RAM, we penalize it further.

        // Let's create a carbon penalty heuristic:
        // Assume baseline max emissions for a very large VM is around 100.0 mg/sec.
        let emission_penalty = (emissions / 100.0).clamp(0.0, 1.0) * 20.0;

        // Base score is mostly efficiency, penalized slightly if raw emissions are massive
        let mut overall_score = efficiency.score - emission_penalty;
        overall_score = overall_score.clamp(0.0, 100.0);

        let mut insights = efficiency.insights.clone();

        if emissions > 50.0 {
            insights.push(format!(
                "High absolute carbon footprint: {emissions:.1} mg/sec."
            ));
        }
        if overall_score < 50.0 {
            insights.push("Poor sustainability score. Workload is wasting energy.".to_string());
        } else if overall_score > 90.0 {
            insights.push(
                "Excellent sustainability score. Resources are optimally utilized.".to_string(),
            );
        }

        EcoScoreResult {
            overall_score,
            efficiency_score: efficiency.score,
            emission_rate_mg_sec: emissions,
            insights,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu: f32, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total,
                used_bytes: mem_used,
                free_bytes: mem_total.saturating_sub(mem_used),
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
    fn test_highly_efficient_eco_score() {
        let snap = dummy_snapshot(90.0, 1024 * 1024 * 1024, 900 * 1024 * 1024);
        let factors = EmissionFactors::new(200.0); // clean grid
        let eco = snap.calculate_eco_score(&factors, 1);

        assert!(eco.overall_score > 85.0);
        assert!(eco.insights.iter().any(|i| i.contains("Excellent")));
    }

    #[test]
    fn test_terrible_eco_score() {
        // Very inefficient, very large VM
        let snap = dummy_snapshot(5.0, 64 * 1024 * 1024 * 1024, 100 * 1024 * 1024);
        let factors = EmissionFactors::new(1800.0); // extremely dirty grid to bump up absolute mg/s
        let eco = snap.calculate_eco_score(&factors, 128); // 128 cores idle

        assert!(eco.overall_score < 40.0);
        assert!(eco.insights.iter().any(|i| i.contains("Poor")));
        assert!(eco.insights.iter().any(|i| i.contains("High absolute")));
    }
}
