//! Eco Advisor Module
//!
//! # Abstract
//! This module combines the `CarbonEstimator` and `RightSizer` to provide
//! a unified `EcoReport`. It analyzes a VM's resource usage to suggest rightsizing
//! actions while quantifying the current carbon footprint, enabling "GreenOps".
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{EcoAdvisor, EmissionFactors, MetricsSnapshot, VmConfig};
//! use hitz_api::{CpuMetrics, MemoryMetrics, GuestAgentMode};
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
//!         total_pct: 5.0,
//!         per_core: vec![5.0; 4],
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
//! let report = snap.generate_eco_report(&config, &factors);
//!
//! assert!(report.current_emissions_mg_per_sec > 0.0);
//! assert!(!report.recommendations.is_empty());
//! ```

use crate::{
    CarbonEstimator, EmissionFactors, MetricsSnapshot, ResizeRecommendation, RightSizer, VmConfig,
};
use serde::{Deserialize, Serialize};

/// A unified report detailing current emissions and rightsizing recommendations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoReport {
    /// The current rate of carbon emissions in mg CO2/sec.
    pub current_emissions_mg_per_sec: f64,
    /// Recommendations for rightsizing the VM to save resources and emissions.
    pub recommendations: Vec<ResizeRecommendation>,
}

/// Trait for generating an ecological and efficiency report.
pub trait EcoAdvisor {
    /// Evaluates metrics to estimate emissions and recommend sizing changes.
    fn generate_eco_report(&self, config: &VmConfig, factors: &EmissionFactors) -> EcoReport;
}

impl EcoAdvisor for MetricsSnapshot {
    fn generate_eco_report(&self, config: &VmConfig, factors: &EmissionFactors) -> EcoReport {
        let current_emissions_mg_per_sec = self.estimate_carbon(factors, config.cpus);
        let recommendations = self.recommend_sizing(config);

        EcoReport {
            current_emissions_mg_per_sec,
            recommendations,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn test_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048,
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
                total_bytes: 2048 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 1948 * 1024 * 1024,
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
    fn test_generate_eco_report() {
        let config = test_config();
        let snap = test_snapshot(5.0); // Low CPU, should trigger scale down
        let factors = EmissionFactors::new(400.0);

        let report = snap.generate_eco_report(&config, &factors);

        assert!(report.current_emissions_mg_per_sec > 0.0);
        assert!(
            !report.recommendations.is_empty(),
            "Should have scale down recommendations"
        );
    }
}
