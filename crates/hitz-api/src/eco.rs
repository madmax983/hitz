//! Eco scoring module for evaluating sustainability.
//!
//! # Abstract
//! Combines resource efficiency with carbon footprint estimation to
//! provide an overall sustainability score for a VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoScorer, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
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
//! let eco_score = snap.calculate_eco_score(&factors, 1);
//! assert!(eco_score.sustainability_index < 100.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// Result of the eco scoring evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoScore {
    /// Sustainability index (0.0 to 100.0). Higher is better.
    pub sustainability_index: f64,
    /// The calculated efficiency score (0.0 to 100.0).
    pub efficiency_score: f64,
    /// Estimated carbon emissions rate (mg CO2/sec).
    pub carbon_emissions_rate: f64,
    /// Insights on how to improve sustainability.
    pub insights: Vec<String>,
}

/// Trait for evaluating the overall sustainability of a workload.
pub trait EcoScorer {
    /// Evaluates sustainability and returns an [`EcoScore`].
    fn calculate_eco_score(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScore;
}

impl EcoScorer for MetricsSnapshot {
    fn calculate_eco_score(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScore {
        let efficiency = self.calculate_efficiency();
        let carbon = self.estimate_carbon(factors, vcpus);

        // Heuristic: If efficiency is low (wasted resources), heavily penalize
        // based on how "dirty" the grid is (represented by grid_intensity).
        let mut sustainability = efficiency.score;
        let mut insights = efficiency.insights.clone();

        // The dirtier the grid, the more we penalize waste.
        // If intensity > 300g/kWh, it's considered dirty.
        if factors.grid_intensity_g_per_kwh > 300.0 && efficiency.score < 80.0 {
            let waste_penalty = (100.0 - efficiency.score) * 0.5;
            sustainability -= waste_penalty;
            insights.push(format!(
                "High emissions grid ({:.0} gCO2/kWh) amplifying efficiency waste penalty.",
                factors.grid_intensity_g_per_kwh
            ));
        }

        EcoScore {
            sustainability_index: sustainability.clamp(0.0, 100.0),
            efficiency_score: efficiency.score,
            carbon_emissions_rate: carbon,
            insights,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu: f32, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: mem_used,
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
    fn test_eco_scorer_dirty_grid_penalty() {
        // Wasting resources: low CPU, low Memory
        let snap = dummy_snapshot(5.0, 10 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0); // Dirty grid
        let score = snap.calculate_eco_score(&factors, 1);

        assert!(score.efficiency_score < 100.0);
        // Should have the extra penalty
        assert!(score.sustainability_index < score.efficiency_score);
        assert!(
            score
                .insights
                .iter()
                .any(|i| i.contains("High emissions grid"))
        );
    }

    #[test]
    fn test_eco_scorer_clean_grid_no_penalty() {
        // Wasting resources
        let snap = dummy_snapshot(5.0, 10 * 1024 * 1024);
        let factors = EmissionFactors::new(150.0); // Clean grid
        let score = snap.calculate_eco_score(&factors, 1);

        assert!(score.efficiency_score < 100.0);
        // No extra penalty
        assert!((score.sustainability_index - score.efficiency_score).abs() < f64::EPSILON);
        assert!(
            !score
                .insights
                .iter()
                .any(|i| i.contains("High emissions grid"))
        );
    }
}
