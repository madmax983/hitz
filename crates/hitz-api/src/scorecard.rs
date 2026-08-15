#![allow(clippy::module_name_repetitions)]
//! Composite scorecard module.
//!
//! # Abstract
//! Generates a holistic system score by combining `EfficiencyScore`, `HealthCheck`, and `CoreImbalanceAnalyzer`.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CompositeScorer, SystemScore, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024 * 1024 * 1024, used_bytes: 512 * 1024 * 1024, free_bytes: 512 * 1024 * 1024, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let score = snap.calculate_composite_score();
//! assert!(score.overall_score <= 100.0);
//! ```

use crate::{
    CoreImbalanceAnalyzer, EfficiencyScore, EfficiencyScorer, HealthCheck, HealthStatus,
    ImbalanceResult, MetricsSnapshot, SystemHealth,
};
use serde::{Deserialize, Serialize};

/// A holistic system score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemScore {
    /// The overall score (0.0 - 100.0).
    pub overall_score: f64,
    /// Efficiency analysis results.
    pub efficiency: EfficiencyScore,
    /// Health assessment results.
    pub health: SystemHealth,
    /// Core imbalance results.
    pub imbalance: ImbalanceResult,
}

/// Trait to calculate a composite score.
pub trait CompositeScorer {
    /// Calculates the composite score.
    fn calculate_composite_score(&self) -> SystemScore;
}

impl CompositeScorer for MetricsSnapshot {
    #[allow(clippy::suboptimal_flops)]
    fn calculate_composite_score(&self) -> SystemScore {
        let efficiency = self.calculate_efficiency();
        let health = self.assess_health();
        let imbalance = self.analyze_imbalance();

        let mut score = efficiency.score;

        // Penalize for health issues
        match health.status {
            HealthStatus::Critical => score -= 30.0,
            HealthStatus::Warning => score -= 15.0,
            HealthStatus::Healthy => {}
        }

        // Penalize for extreme imbalance
        if imbalance.is_imbalanced {
            score = (-10.0_f64).mul_add(imbalance.imbalance_score, score);
        }

        SystemScore {
            overall_score: score.clamp(0.0, 100.0),
            efficiency,
            health,
            imbalance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 4 * 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
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
    fn test_calculate_composite_score() {
        let snap = dummy_snapshot();
        let score = snap.calculate_composite_score();
        assert!(score.overall_score >= 0.0);
        assert!(score.overall_score <= 100.0);
    }
}
