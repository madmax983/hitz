//! Sustainability Reporter for VM Metrics.
//!
//! # Abstract
//! This module combines the `CarbonEstimator`, `EfficiencyScorer`, and `RightSizer`
//! into a single comprehensive `SustainabilityReport`.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode, SustainabilityReporter, SustainabilityReport, EmissionFactors};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0; 4],
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
//! let factors = EmissionFactors::new(400.0);
//! let report = snap.generate_sustainability_report(&config, &factors);
//! assert!(report.efficiency_score.score < 100.0);
//! ```

use crate::carbon::{CarbonEstimator, EmissionFactors};
use crate::efficiency::{EfficiencyScore, EfficiencyScorer};
use crate::rightsizer::{ResizeRecommendation, RightSizer};
use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Comprehensive sustainability report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SustainabilityReport {
    /// Estimated carbon emission rate in mg CO2/sec.
    pub carbon_emission_mg_per_sec: f64,
    /// Resource efficiency score and insights.
    pub efficiency_score: EfficiencyScore,
    /// Actionable recommendations to improve resource usage.
    pub resize_recommendations: Vec<ResizeRecommendation>,
}

/// Trait for objects that can generate a sustainability report.
pub trait SustainabilityReporter {
    /// Generates a comprehensive sustainability report.
    fn generate_sustainability_report(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> SustainabilityReport;
}

impl SustainabilityReporter for MetricsSnapshot {
    fn generate_sustainability_report(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> SustainabilityReport {
        let carbon_emission_mg_per_sec = self.estimate_carbon(factors, config.cpus);
        let efficiency_score = self.calculate_efficiency();
        let resize_recommendations = self.recommend_sizing(config);

        SustainabilityReport {
            carbon_emission_mg_per_sec,
            efficiency_score,
            resize_recommendations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_sustainability_report() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0; 4],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 900 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let factors = EmissionFactors::new(400.0);
        let report = snap.generate_sustainability_report(&config, &factors);

        assert!(report.carbon_emission_mg_per_sec > 0.0);
        assert!(report.efficiency_score.score < 100.0);
        assert!(!report.resize_recommendations.is_empty());
    }
}
