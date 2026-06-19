//! Sustainability Module.
//!
//! # Abstract
//! Combines the `CarbonEstimator` and `EfficiencyScorer` into a unified `SustainabilityReport`.
//! This allows administrators to evaluate not only the raw emissions of a VM, but also how
//! *justified* those emissions are based on the actual resource utilization efficiency.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{SustainabilityReporter, SustainabilityReport, MetricsSnapshot, CpuMetrics, MemoryMetrics, EmissionFactors};
//!
//! // Provide a generic snapshot
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
//! let report = snap.generate_sustainability_report(&factors, 1);
//!
//! assert!(report.carbon_emissions_mg_per_sec > 0.0);
//! assert!(report.efficiency_score.score < 100.0);
//! ```

use crate::{CarbonEstimator, EfficiencyScore, EfficiencyScorer, EmissionFactors, MetricsSnapshot};
use serde::{Deserialize, Serialize};

/// A combined report showing real-time carbon footprint and resource utilization efficiency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SustainabilityReport {
    /// The estimated current carbon emission rate in mg CO2/sec.
    pub carbon_emissions_mg_per_sec: f64,
    /// The efficiency score of the current resource usage.
    pub efficiency_score: EfficiencyScore,
    /// Recommendations to improve sustainability, prioritizing reduction of waste.
    pub recommendations: Vec<String>,
}

/// Trait to generate a comprehensive sustainability report.
pub trait SustainabilityReporter {
    /// Generates the `SustainabilityReport`.
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
        let emissions = self.estimate_carbon(factors, vcpus);
        let efficiency = self.calculate_efficiency();

        let mut recommendations = Vec::new();
        if efficiency.score < 80.0 {
            recommendations.push(
                "High carbon footprint due to wasted resources. Consider resizing.".to_string(),
            );
            recommendations.extend(efficiency.insights.clone());
        } else {
            recommendations
                .push("Running efficiently. Emissions are justified by utilization.".to_string());
        }

        SustainabilityReport {
            carbon_emissions_mg_per_sec: emissions,
            efficiency_score: efficiency,
            recommendations,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn test_snapshot(cpu: f32, ram_bytes: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
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
    fn test_sustainability_report_high_waste() {
        let factors = EmissionFactors::new(400.0);
        // Extremely low usage on large allocation
        let snap = test_snapshot(5.0, 8 * 1024 * 1024 * 1024, 100 * 1024 * 1024);
        let report = snap.generate_sustainability_report(&factors, 4);

        assert!(report.carbon_emissions_mg_per_sec > 0.0);
        assert!(report.efficiency_score.score < 50.0);
        assert!(!report.recommendations.is_empty());
        assert!(
            report
                .recommendations
                .iter()
                .any(|r| r.contains("High carbon footprint due to wasted resources"))
        );
    }

    #[test]
    fn test_sustainability_report_highly_efficient() {
        let factors = EmissionFactors::new(400.0);
        // High usage on large allocation
        let snap = test_snapshot(90.0, 8 * 1024 * 1024 * 1024, 7 * 1024 * 1024 * 1024);
        let report = snap.generate_sustainability_report(&factors, 4);

        assert!(report.carbon_emissions_mg_per_sec > 0.0);
        assert!(report.efficiency_score.score > 80.0);
        // Efficient VMs shouldn't have waste warnings, maybe general green advice
        assert!(
            report.recommendations.is_empty()
                || report
                    .recommendations
                    .iter()
                    .any(|r| r.contains("Running efficiently"))
        );
    }
}
