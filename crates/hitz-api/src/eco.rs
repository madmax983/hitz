//! Ecological impact analysis module.
//!
//! # Abstract
//! This module combines the resource efficiency calculations (`EfficiencyScorer`)
//! with the absolute carbon footprint estimation (`CarbonEstimator`) to evaluate
//! the *wasted* carbon emissions of a VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoAnalyzer, EcoScore, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0, 10.0, 10.0, 10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 3 * 1024 * 1024 * 1024,
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
//! let factors = EmissionFactors::new(400.0); // 400 gCO2eq/kWh
//! let score = snap.analyze_eco(&factors, 4);
//!
//! // With low efficiency, a significant portion of the footprint is wasted.
//! assert!(score.wasted_emissions_mg_per_sec > 0.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// The computed ecological impact score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoScore {
    /// The absolute carbon emission rate in mg CO2/sec.
    pub total_emissions_mg_per_sec: f64,
    /// The portion of emissions considered "wasted" due to inefficiency (mg CO2/sec).
    pub wasted_emissions_mg_per_sec: f64,
    /// A normalized eco-efficiency rating (0.0 = terrible, 100.0 = perfect).
    pub eco_rating: f64,
}

/// Trait to analyze ecological impact based on efficiency and footprint.
pub trait EcoAnalyzer {
    /// Analyzes the ecological impact of the current state.
    fn analyze_eco(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScore;
}

impl EcoAnalyzer for MetricsSnapshot {
    fn analyze_eco(&self, factors: &EmissionFactors, vcpus: u32) -> EcoScore {
        // Step 1: Calculate raw efficiency score (0.0 to 100.0)
        let efficiency = self.calculate_efficiency();

        // Step 2: Estimate absolute total emissions (mg CO2 / sec)
        let total_emissions = self.estimate_carbon(factors, vcpus);

        // Step 3: Calculate waste
        // If efficiency is 100%, waste is 0. If efficiency is 20%, waste is 80% of total.
        let inefficiency_pct = (100.0 - efficiency.score) / 100.0;
        let wasted_emissions = total_emissions * inefficiency_pct;

        EcoScore {
            total_emissions_mg_per_sec: total_emissions,
            wasted_emissions_mg_per_sec: wasted_emissions,
            eco_rating: efficiency.score,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu: f32, mem_used_gb: u64, mem_total_gb: u64) -> MetricsSnapshot {
        let total_bytes = mem_total_gb * 1024 * 1024 * 1024;
        let used_bytes = mem_used_gb * 1024 * 1024 * 1024;
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes,
                used_bytes,
                free_bytes: total_bytes.saturating_sub(used_bytes),
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
    fn test_perfect_efficiency_no_waste() {
        let factors = EmissionFactors::new(400.0);
        // 90% CPU, 90% Memory -> High efficiency
        let snap = dummy_snapshot(90.0, 3, 4);
        let score = snap.analyze_eco(&factors, 4);

        assert!(score.total_emissions_mg_per_sec > 0.0);
        assert_eq!(score.wasted_emissions_mg_per_sec, 0.0);
        assert_eq!(score.eco_rating, 100.0);
    }

    #[test]
    fn test_poor_efficiency_high_waste() {
        let factors = EmissionFactors::new(400.0);
        // 5% CPU, 10% Memory -> Low efficiency (score will be < 50)
        let snap = dummy_snapshot(5.0, 1, 10);
        let score = snap.analyze_eco(&factors, 4);

        assert!(score.total_emissions_mg_per_sec > 0.0);
        assert!(score.wasted_emissions_mg_per_sec > 0.0);
        assert!(score.wasted_emissions_mg_per_sec < score.total_emissions_mg_per_sec);
        assert!(score.eco_rating < 50.0);
    }
}
