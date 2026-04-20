//! Advisor module for providing actionable recommendations.
//!
//! # Abstract
//! This module combines the `CarbonEstimator` and `EfficiencyScorer` to provide
//! actionable recommendations to the user, suggesting scaling actions to improve
//! sustainability and reduce wasted resources.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{Advisor, Advice, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 0,
//!         buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
//!     },
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let factors = EmissionFactors::new(400.0);
//! let advice = snap.advise(&factors, 1);
//! assert_eq!(advice.action, "Scale Down");
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// Actionable advice derived from metrics and efficiency scores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    /// Recommended action (e.g., "Scale Down", "Scale Up", "Maintain").
    pub action: String,
    /// Human-readable explanation for the recommendation.
    pub reason: String,
    /// Estimated impact on carbon emissions in mg CO2/sec if the action is taken.
    pub estimated_carbon_savings_mg_per_sec: f64,
}

/// Trait for objects that can provide actionable advice.
pub trait Advisor {
    /// Generates advice based on the current state.
    fn advise(&self, factors: &EmissionFactors, vcpus: u32) -> Advice;
}

impl Advisor for MetricsSnapshot {
    fn advise(&self, factors: &EmissionFactors, vcpus: u32) -> Advice {
        let efficiency = self.calculate_efficiency();
        let emissions = self.estimate_carbon(factors, vcpus);

        if efficiency.score < 50.0 {
            Advice {
                action: "Scale Down".to_string(),
                reason: format!(
                    "Efficiency is low ({:.1}/100). Downscaling will reduce waste.",
                    efficiency.score
                ),
                estimated_carbon_savings_mg_per_sec: emissions * 0.4, // Assume 40% savings
            }
        } else if efficiency.score > 90.0 {
            Advice {
                action: "Scale Up".to_string(),
                reason: format!(
                    "System is under heavy load ({:.1}/100). Upscaling recommended.",
                    efficiency.score
                ),
                estimated_carbon_savings_mg_per_sec: emissions * -0.2, // Will cost more carbon
            }
        } else {
            Advice {
                action: "Maintain".to_string(),
                reason: format!(
                    "System is running efficiently ({:.1}/100).",
                    efficiency.score
                ),
                estimated_carbon_savings_mg_per_sec: 0.0,
            }
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
    fn test_advise_scale_down() {
        let snap = dummy_snapshot(5.0, 4 * 1024 * 1024 * 1024, 100 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);
        let advice = snap.advise(&factors, 4);

        assert_eq!(advice.action, "Scale Down");
        assert!(advice.estimated_carbon_savings_mg_per_sec > 0.0);
    }

    #[test]
    fn test_advise_scale_up() {
        let snap = dummy_snapshot(95.0, 1024 * 1024 * 1024, 950 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);
        let advice = snap.advise(&factors, 1);

        assert_eq!(advice.action, "Scale Up");
        assert!(advice.estimated_carbon_savings_mg_per_sec < 0.0);
    }

    #[test]
    fn test_advise_maintain() {
        // Use optimal values to get efficiency between 50 and 90
        let snap = dummy_snapshot(60.0, 2 * 1024 * 1024 * 1024, 1500 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);
        let advice = snap.advise(&factors, 2);

        assert_eq!(advice.action, "Maintain");
        assert!(advice.estimated_carbon_savings_mg_per_sec.abs() < f64::EPSILON);
    }
}
