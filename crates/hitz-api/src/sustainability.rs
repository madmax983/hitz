//! Sustainability Reporting module.
//!
//! # Abstract
//! This module combines the `CarbonEstimator` and `EfficiencyScorer` into a single,
//! holistic `SustainabilityReport`. It allows users to quickly determine if a VM
//! is both cost-effective (efficient) and environmentally friendly (low carbon).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{SustainabilityReporter, MetricsSnapshot, CpuMetrics, MemoryMetrics, EmissionFactors};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 900 * 1024 * 1024,
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
//! let report = snap.generate_sustainability_report(&factors, 1);
//! assert!(report.efficiency_score < 50.0);
//! assert!(report.emissions_mg_per_sec > 0.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// A combined report of efficiency and carbon emissions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SustainabilityReport {
    /// The efficiency score (0.0 to 100.0).
    pub efficiency_score: f64,
    /// The estimated carbon emissions in mg CO2/sec.
    pub emissions_mg_per_sec: f64,
}

/// Trait to generate a sustainability report.
pub trait SustainabilityReporter {
    /// Generates a report combining efficiency and carbon estimates.
    fn generate_sustainability_report(
        &self,
        factors: &EmissionFactors,
        vcpus: u32,
    ) -> SustainabilityReport;
}

impl SustainabilityReporter for MetricsSnapshot {
    fn generate_sustainability_report(
        &self,
        factors: &EmissionFactors,
        vcpus: u32,
    ) -> SustainabilityReport {
        let efficiency = self.calculate_efficiency();
        let emissions = self.estimate_carbon(factors, vcpus);

        SustainabilityReport {
            efficiency_score: efficiency.score,
            emissions_mg_per_sec: emissions,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_sustainability_reporter() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0],
                load_avg: [0.1, 0.1, 0.1],
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
        let report = snap.generate_sustainability_report(&factors, 1);

        assert!(report.efficiency_score > 0.0);
        assert!(report.emissions_mg_per_sec > 0.0);
    }
}
