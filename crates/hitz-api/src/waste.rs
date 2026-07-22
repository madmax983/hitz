//! Carbon Waste Estimator module.
//!
//! # Abstract
//! This module connects the `CarbonEstimator` and `EfficiencyScorer` to
//! calculate the amount of carbon emissions that are effectively "wasted"
//! due to underutilization of resources.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CarbonWasteEstimator, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
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
//! let waste = snap.estimate_waste(&factors, 4);
//! assert!(waste > 0.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};

/// Trait to estimate carbon waste.
pub trait CarbonWasteEstimator {
    /// Estimates the current rate of wasted carbon emissions in milligrams of CO2 per second (mg CO2/sec).
    fn estimate_waste(&self, factors: &EmissionFactors, vcpus: u32) -> f64;
}

impl CarbonWasteEstimator for MetricsSnapshot {
    fn estimate_waste(&self, factors: &EmissionFactors, vcpus: u32) -> f64 {
        let total_emissions = self.estimate_carbon(factors, vcpus);
        let efficiency = self.calculate_efficiency();

        let waste_pct = (100.0 - efficiency.score) / 100.0;
        total_emissions * waste_pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(cpu_pct: f32, ram_bytes: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_bytes,
                used_bytes: mem_used,
                free_bytes: ram_bytes.saturating_sub(mem_used),
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
    fn test_zero_waste_when_efficient() {
        let factors = EmissionFactors::new(400.0);
        let snap = base_snapshot(90.0, 1024 * 1024 * 1024, 900 * 1024 * 1024);

        let emissions = snap.estimate_carbon(&factors, 4);
        let waste = snap.estimate_waste(&factors, 4);

        assert!(emissions > 0.0);
        assert_eq!(waste, 0.0); // 100% efficient -> 0 waste
    }

    #[test]
    fn test_high_waste_when_inefficient() {
        let factors = EmissionFactors::new(400.0);
        let snap = base_snapshot(5.0, 4 * 1024 * 1024 * 1024, 100 * 1024 * 1024);

        let emissions = snap.estimate_carbon(&factors, 4);
        let waste = snap.estimate_waste(&factors, 4);

        assert!(emissions > 0.0);
        assert!(waste > 0.0);
        assert!(waste < emissions);
    }
}
