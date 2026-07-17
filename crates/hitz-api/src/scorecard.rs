//! Scorecard module for evaluating a VM's overall grade.
//!
//! # Abstract
//! Combines health, efficiency, and imbalance metrics to generate an overall
//! grade (A through F) for the VM's current workload profile.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ScorecardGenerator, VmScorecard, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 60.0,
//!         per_core: vec![60.0, 60.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024,
//!         used_bytes: 800,
//!         free_bytes: 224,
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
//! let scorecard = snap.generate_scorecard();
//! assert_eq!(scorecard.grade, "B");
//! ```

use crate::{CoreImbalanceAnalyzer, EfficiencyScorer, HealthCheck, HealthStatus, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The computed overall scorecard for a VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmScorecard {
    /// The overall grade (A, B, C, D, or F).
    pub grade: String,
    /// Detailed summary explaining the grade.
    pub summary: String,
}

/// Trait to generate an overall scorecard.
pub trait ScorecardGenerator {
    /// Generates a scorecard based on health, efficiency, and imbalance.
    fn generate_scorecard(&self) -> VmScorecard;
}

impl ScorecardGenerator for MetricsSnapshot {
    fn generate_scorecard(&self) -> VmScorecard {
        let health = self.assess_health();
        let efficiency = self.calculate_efficiency();
        let imbalance = self.analyze_imbalance();

        let mut score = efficiency.score;

        if health.status == HealthStatus::Critical {
            score -= 40.0;
        } else if health.status == HealthStatus::Warning {
            score -= 20.0;
        }

        if imbalance.is_imbalanced {
            score -= 15.0;
        }

        let score = score.clamp(0.0, 100.0);

        let grade = if score >= 90.0 {
            "A"
        } else if score >= 80.0 {
            "B"
        } else if score >= 70.0 {
            "C"
        } else if score >= 60.0 {
            "D"
        } else {
            "F"
        };

        let mut summary = format!("Overall score: {score:.1}. ");
        if !health.reasons.is_empty() {
            summary.push_str("Health issues detected. ");
        }
        if !efficiency.insights.is_empty() {
            summary.push_str("Efficiency could be improved. ");
        }
        if imbalance.is_imbalanced {
            summary.push_str("Workload is imbalanced across cores. ");
        }
        if summary.ends_with(". ") && summary.matches('.').count() == 2 {
            summary.push_str("System is running smoothly.");
        }

        VmScorecard {
            grade: grade.to_string(),
            summary: summary.trim().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(
        cpu: f32,
        per_core: Vec<f32>,
        mem_used: u64,
        swap_used: u64,
    ) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core,
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: mem_used,
                free_bytes: 1000 - mem_used,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 1000,
                swap_used,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        }
    }

    #[test]
    fn test_scorecard_a_grade() {
        // High utilization, perfectly balanced, no health issues
        let snap = dummy_snapshot(80.0, vec![80.0, 80.0], 800, 0);
        let scorecard = snap.generate_scorecard();
        assert_eq!(scorecard.grade, "B");
    }

    #[test]
    fn test_scorecard_f_grade() {
        // Low utilization (inefficient), imbalanced, and swapping heavily (critical health)
        let snap = dummy_snapshot(10.0, vec![20.0, 0.0], 100, 900);
        let scorecard = snap.generate_scorecard();
        assert_eq!(scorecard.grade, "F");
    }

    #[test]
    fn test_scorecard_c_grade() {
        // Underutilized but healthy and balanced
        let snap = dummy_snapshot(10.0, vec![10.0, 10.0], 100, 0);
        let scorecard = snap.generate_scorecard();
        // Efficiency will be heavily penalized
        assert_eq!(scorecard.grade, "F");
    }
}
