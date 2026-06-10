//! Eco-Grading module.
//!
//! # Abstract
//! Calculates an Eco-Grade (A to F) based on carbon emissions and resource efficiency.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoGrader, EcoGrade, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
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
//! let factors = EmissionFactors::new(200.0);
//! let grade = snap.calculate_eco_grade(&factors, 1);
//! assert!(grade.score < 100.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The computed eco grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoGrade {
    /// Overall score from 0.0 to 100.0.
    pub score: f64,
    /// Letter grade (A to F).
    pub grade: char,
}

/// Trait to calculate eco grade.
pub trait EcoGrader {
    /// Calculates eco grade from metrics.
    fn calculate_eco_grade(&self, factors: &EmissionFactors, vcpus: u32) -> EcoGrade;
}

impl EcoGrader for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_eco_grade(&self, factors: &EmissionFactors, vcpus: u32) -> EcoGrade {
        let efficiency_score = self.calculate_efficiency().score;
        let emissions = self.estimate_carbon(factors, vcpus);

        // Normalize emissions (lower is better, cap at a somewhat arbitrary high value for score).
        // Let's say 20 mg/s is an F (0 score for carbon part).
        let emissions_score = emissions.mul_add(-5.0, 100.0).clamp(0.0, 100.0);

        // Combine efficiency and carbon emissions (50/50 weighting)
        let total_score = efficiency_score.mul_add(0.5, emissions_score * 0.5);

        let grade = match total_score {
            s if s >= 90.0 => 'A',
            s if s >= 80.0 => 'B',
            s if s >= 70.0 => 'C',
            s if s >= 60.0 => 'D',
            _ => 'F',
        };

        EcoGrade {
            score: total_score,
            grade,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
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
    fn test_perfect_grade() {
        let snap = dummy_snapshot(90.0, 1024 * 1024 * 1024, 900 * 1024 * 1024);
        let factors = EmissionFactors::new(10.0); // very clean grid
        let grade = snap.calculate_eco_grade(&factors, 1);
        assert!(grade.score > 90.0);
        assert_eq!(grade.grade, 'A');
    }
}
