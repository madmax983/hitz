//! Eco-aware VM Rightsizing.
//!
//! # Abstract
//! This module combines the `RightSizer` and `CarbonEstimator` to provide
//! actionable scaling recommendations alongside their estimated environmental impact.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoSizer, EmissionFactors, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 2048,
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
//!         total_pct: 5.0, // Very low usage
//!         per_core: vec![5.0; 4],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 2048 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 1948 * 1024 * 1024,
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
//! let eco_recs = snap.recommend_eco_sizing(&config, &factors);
//!
//! assert!(!eco_recs.is_empty());
//! // Recommends scaling down, showing carbon savings (negative delta).
//! assert!(eco_recs[0].carbon_delta_mg_per_sec < 0.0);
//! ```

use crate::{
    CarbonEstimator, EmissionFactors, MetricsSnapshot, ResizeRecommendation, RightSizer, VmConfig,
};
use serde::{Deserialize, Serialize};

/// A resizing recommendation augmented with its estimated carbon impact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoRecommendation {
    /// The underlying resource scaling recommendation.
    pub recommendation: ResizeRecommendation,
    /// The estimated change in carbon emissions (mg CO2/sec).
    /// A negative value represents carbon savings (e.g. from scaling down).
    pub carbon_delta_mg_per_sec: f64,
}

/// Trait for objects that can evaluate metrics and recommend resizing with environmental impact.
pub trait EcoSizer {
    /// Analyzes current metrics against the VM's configuration and returns eco-aware recommendations.
    fn recommend_eco_sizing(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> Vec<EcoRecommendation>;
}

impl EcoSizer for MetricsSnapshot {
    fn recommend_eco_sizing(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> Vec<EcoRecommendation> {
        let base_recs = self.recommend_sizing(config);
        let mut eco_recs = Vec::with_capacity(base_recs.len());

        let current_emissions = self.estimate_carbon(factors, config.cpus);

        for rec in base_recs {
            let carbon_delta_mg_per_sec = match &rec {
                ResizeRecommendation::ScaleUpCpu { suggested, .. }
                | ResizeRecommendation::ScaleDownCpu { suggested, .. } => {
                    let mut simulated_snap = self.clone();
                    // Adjust total_pct to maintain the same total compute load.
                    #[allow(clippy::cast_precision_loss)]
                    let current_core_eq =
                        (simulated_snap.cpu.total_pct / 100.0) * (config.cpus as f32);
                    #[allow(clippy::cast_precision_loss)]
                    let new_pct = (current_core_eq / (*suggested as f32)) * 100.0;
                    simulated_snap.cpu.total_pct = new_pct.clamp(0.0, 100.0);

                    let simulated_emissions = simulated_snap.estimate_carbon(factors, *suggested);
                    simulated_emissions - current_emissions
                }
                ResizeRecommendation::ScaleUpRam { suggested_mib, .. }
                | ResizeRecommendation::ScaleDownRam { suggested_mib, .. } => {
                    let mut simulated_snap = self.clone();
                    simulated_snap.memory.total_bytes = u64::from(*suggested_mib) * 1024 * 1024;
                    let simulated_emissions = simulated_snap.estimate_carbon(factors, config.cpus);
                    simulated_emissions - current_emissions
                }
            };

            eco_recs.push(EcoRecommendation {
                recommendation: rec,
                carbon_delta_mg_per_sec,
            });
        }

        eco_recs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    fn test_config(cpus: u32, ram_mib: u32) -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib,
            cpus,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    fn test_snapshot(cpu_pct: f32, ram_mib: u32, mem_used_mib: u32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: u64::from(ram_mib) * 1024 * 1024,
                used_bytes: u64::from(mem_used_mib) * 1024 * 1024,
                free_bytes: 0,
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
    fn test_eco_scale_down_saves_carbon() {
        let config = test_config(4, 2048);
        // Very low CPU (5%) and Memory usage (~9%)
        let snap = test_snapshot(5.0, 2048, 200);
        let factors = EmissionFactors::new(400.0);

        let eco_recs = snap.recommend_eco_sizing(&config, &factors);

        assert_eq!(eco_recs.len(), 2);

        for eco_rec in eco_recs {
            assert!(
                eco_rec.carbon_delta_mg_per_sec < 0.0,
                "Scaling down should result in negative carbon delta (savings), got {}",
                eco_rec.carbon_delta_mg_per_sec
            );
        }
    }

    #[test]
    fn test_eco_scale_up_increases_carbon() {
        let config = test_config(2, 512);
        // Very high CPU (95%) and Memory usage (~97%)
        let snap = test_snapshot(95.0, 512, 500);
        let factors = EmissionFactors::new(400.0);

        let eco_recs = snap.recommend_eco_sizing(&config, &factors);

        assert_eq!(eco_recs.len(), 2);

        for eco_rec in eco_recs {
            assert!(
                eco_rec.carbon_delta_mg_per_sec > 0.0,
                "Scaling up should result in positive carbon delta, got {}",
                eco_rec.carbon_delta_mg_per_sec
            );
        }
    }
}
