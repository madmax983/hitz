//! Scorecard module for unified VM grading.
//!
//! # Abstract
//! This module combines health, efficiency, and imbalance metrics to generate
//! a single, holistic grade (A-F) for a VM's current operational state.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{ScorecardGenerator, VmScorecard, LetterGrade, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 50.0, per_core: vec![50.0, 50.0], load_avg: [1.0, 1.0, 1.0] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 700, free_bytes: 324, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let scorecard = snap.generate_scorecard();
//! assert_eq!(scorecard.grade, LetterGrade::A);
//! ```

use crate::{CoreImbalanceAnalyzer, EfficiencyScorer, HealthCheck, HealthStatus, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The overall grade of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LetterGrade {
    /// Excellent health and efficiency.
    A,
    /// Good health and efficiency.
    B,
    /// Average health and efficiency.
    C,
    /// Poor health and efficiency.
    D,
    /// Failing health and efficiency.
    F,
}

/// A unified scorecard for a VM's operational state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmScorecard {
    /// The final letter grade.
    pub grade: LetterGrade,
    /// The final numerical score (0.0 to 100.0).
    pub total_score: f64,
    /// The underlying health status.
    pub health_status: HealthStatus,
    /// The underlying efficiency score.
    pub efficiency_score: f64,
    /// The underlying imbalance score.
    pub imbalance_score: f64,
    /// Actionable insights derived from the evaluation.
    pub insights: Vec<String>,
}

/// Trait to generate a unified scorecard.
pub trait ScorecardGenerator {
    /// Generates a unified scorecard from the underlying metrics.
    fn generate_scorecard(&self) -> VmScorecard;
}

impl ScorecardGenerator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn generate_scorecard(&self) -> VmScorecard {
        let health = self.assess_health();
        let efficiency = self.calculate_efficiency();
        let imbalance = self.analyze_imbalance();

        let mut total_score = efficiency.score;
        let mut combined_insights = efficiency.insights.clone();

        if health.status == HealthStatus::Critical {
            total_score -= 40.0;
            combined_insights.push("Critical health issues detected.".to_string());
        } else if health.status == HealthStatus::Warning {
            total_score -= 20.0;
            combined_insights.push("Warning health issues detected.".to_string());
        }

        if imbalance.is_imbalanced {
            let penalty = imbalance.imbalance_score * 20.0;
            total_score -= penalty;
            combined_insights.push(format!(
                "Core imbalance detected (Score: {:.2}).",
                imbalance.imbalance_score
            ));
        }

        let final_score = total_score.clamp(0.0, 100.0);

        let grade = if final_score >= 90.0 {
            LetterGrade::A
        } else if final_score >= 80.0 {
            LetterGrade::B
        } else if final_score >= 70.0 {
            LetterGrade::C
        } else if final_score >= 60.0 {
            LetterGrade::D
        } else {
            LetterGrade::F
        };

        VmScorecard {
            grade,
            total_score: final_score,
            health_status: health.status,
            efficiency_score: efficiency.score,
            imbalance_score: imbalance.imbalance_score,
            insights: combined_insights,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(cpu_total: f32, per_core: Vec<f32>, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_total,
                per_core,
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: mem_used,
                free_bytes: 1024 * 1024 * 1024 - mem_used,
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
    fn test_perfect_score() {
        let snap = base_snapshot(60.0, vec![60.0, 60.0], 700 * 1024 * 1024);
        let scorecard = snap.generate_scorecard();
        assert_eq!(scorecard.grade, LetterGrade::A);
        assert_eq!(scorecard.health_status, HealthStatus::Healthy);
    }

    #[test]
    fn test_failing_health() {
        let snap = base_snapshot(95.0, vec![95.0, 95.0], 700 * 1024 * 1024);
        let scorecard = snap.generate_scorecard();
        assert!(scorecard.total_score <= 60.0);
        assert_eq!(scorecard.grade, LetterGrade::D);
    }
}
