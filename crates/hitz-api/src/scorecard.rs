//! Executive Scorecard module for providing a holistic 360-degree view of a VM.
//!
//! # Abstract
//! This module mashes up the `health`, `efficiency`, `rightsizer`, and `carbon` modules
//! to generate a single `VmScorecard`. It's the ultimate executive summary for a running VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ScorecardGenerator, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use hitz_api::{SystemHealth, HealthStatus, EfficiencyScore, ResizeRecommendation, EmissionFactors};
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
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 512 * 1024 * 1024,
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
//! let scorecard = snap.generate_scorecard(&config, &factors);
//!
//! assert_eq!(scorecard.health.status, HealthStatus::Healthy);
//! assert!(scorecard.carbon_emissions_mg_sec > 0.0);
//! ```

use crate::{
    CarbonEstimator, EfficiencyScore, EfficiencyScorer, EmissionFactors, HealthCheck,
    MetricsSnapshot, ResizeRecommendation, RightSizer, SystemHealth, VmConfig,
};
use serde::{Deserialize, Serialize};

/// The ultimate executive summary of a VM's operational state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmScorecard {
    /// The current health status of the VM (e.g., CPU/Memory exhaustion).
    pub health: SystemHealth,
    /// The efficiency score of the VM (are resources being wasted?).
    pub efficiency: EfficiencyScore,
    /// Real-time estimated carbon emissions in mg CO2/sec.
    pub carbon_emissions_mg_sec: f64,
    /// Actionable recommendations to resize the VM.
    pub recommendations: Vec<ResizeRecommendation>,
}

/// Trait to generate a holistic VM scorecard.
pub trait ScorecardGenerator {
    /// Generates a comprehensive scorecard summarizing health, efficiency, carbon, and sizing.
    fn generate_scorecard(
        &self,
        config: &VmConfig,
        emission_factors: &EmissionFactors,
    ) -> VmScorecard;
}

impl ScorecardGenerator for MetricsSnapshot {
    fn generate_scorecard(
        &self,
        config: &VmConfig,
        emission_factors: &EmissionFactors,
    ) -> VmScorecard {
        let health = self.assess_health();
        let efficiency = self.calculate_efficiency();
        let carbon_emissions_mg_sec = self.estimate_carbon(emission_factors, config.cpus);
        let recommendations = self.recommend_sizing(config);

        VmScorecard {
            health,
            efficiency,
            carbon_emissions_mg_sec,
            recommendations,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, HealthStatus, MemoryMetrics};
    use std::path::PathBuf;

    fn test_config() -> VmConfig {
        VmConfig {
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
        }
    }

    fn test_snapshot(cpu_pct: f32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct; 4],
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
        }
    }

    #[test]
    fn test_scorecard_generation_healthy() {
        let config = test_config();
        let snap = test_snapshot(50.0);
        let factors = EmissionFactors::new(400.0);

        let scorecard = snap.generate_scorecard(&config, &factors);

        assert_eq!(scorecard.health.status, HealthStatus::Healthy);
        assert!(scorecard.efficiency.score > 80.0);
        assert!(scorecard.carbon_emissions_mg_sec > 0.0);
        assert!(scorecard.recommendations.is_empty());
    }

    #[test]
    fn test_scorecard_generation_critical() {
        let config = test_config();
        let snap = test_snapshot(95.0); // Spiked CPU
        let factors = EmissionFactors::new(400.0);

        let scorecard = snap.generate_scorecard(&config, &factors);

        assert_eq!(scorecard.health.status, HealthStatus::Critical);
        assert!(scorecard.efficiency.score > 80.0); // Highly efficient, but unhealthy
        assert!(scorecard.carbon_emissions_mg_sec > 0.0);
        assert!(!scorecard.recommendations.is_empty());
    }
}
