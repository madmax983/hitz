//! EcoSizer module for estimating carbon savings from rightsizing recommendations.
//!
//! # Abstract
//! This module combines the `CarbonEstimator` and `RightSizer` traits to
//! provide actionable rightsizing recommendations that include estimated
//! carbon footprint reductions (or increases) based on grid emission factors.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{EcoSizer, EcoRecommendation, EmissionFactors, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 4096,
//!     cpus: 8,
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
//!         per_core: vec![5.0; 8],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4096 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 3584 * 1024 * 1024,
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
//! let recommendations = snap.recommend_eco_sizing(&config, &factors);
//! assert!(!recommendations.is_empty());
//! ```

use crate::{
    CarbonEstimator, EmissionFactors, MetricsSnapshot, ResizeRecommendation, RightSizer, VmConfig,
};
use serde::{Deserialize, Serialize};

/// A resizing recommendation that includes estimated carbon impact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoRecommendation {
    /// The base recommendation from the Rightsizer.
    pub recommendation: ResizeRecommendation,
    /// The estimated change in carbon emissions (mg CO2eq/sec).
    /// Negative means carbon savings (emission reduction).
    /// Positive means increased carbon emissions.
    pub carbon_delta_mg_per_sec: f64,
}

/// Trait for objects that can evaluate metrics and recommend resizing with carbon impact.
pub trait EcoSizer {
    /// Analyzes current metrics against the VM's configuration and emission factors
    /// to return sizing recommendations alongside their estimated carbon impact.
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
        let base_recommendations = self.recommend_sizing(config);
        let current_carbon = self.estimate_carbon(factors, config.cpus);

        let mut eco_recs = Vec::with_capacity(base_recommendations.len());

        for rec in base_recommendations {
            let simulated_carbon = match &rec {
                ResizeRecommendation::ScaleUpCpu { suggested, .. }
                | ResizeRecommendation::ScaleDownCpu { suggested, .. } => {
                    self.estimate_carbon(factors, *suggested)
                }
                ResizeRecommendation::ScaleUpRam { suggested_mib, .. }
                | ResizeRecommendation::ScaleDownRam { suggested_mib, .. } => {
                    // RAM only impacts the total memory bytes in the formula. We clone the snapshot and modify memory to simulate.
                    let mut simulated_snap = self.clone();
                    simulated_snap.memory.total_bytes = u64::from(*suggested_mib) * 1024 * 1024;
                    simulated_snap.estimate_carbon(factors, config.cpus)
                }
            };

            let carbon_delta_mg_per_sec = simulated_carbon - current_carbon;

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

    fn test_snapshot(cpu_pct: f32, ram_bytes: u64, used_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_bytes,
                used_bytes,
                free_bytes: ram_bytes.saturating_sub(used_bytes),
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
    fn test_eco_scale_down_cpu() {
        let config = test_config(4, 2048);
        // Idle CPU (5%), Low RAM (10%)
        let snap = test_snapshot(5.0, 2048 * 1024 * 1024, 204 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);

        let recs = snap.recommend_eco_sizing(&config, &factors);
        assert!(!recs.is_empty());

        let mut found_cpu_down = false;
        for rec in &recs {
            if let ResizeRecommendation::ScaleDownCpu { suggested, .. } = rec.recommendation {
                assert_eq!(suggested, 3);
                assert!(
                    rec.carbon_delta_mg_per_sec < 0.0,
                    "Scaling down CPU should result in negative carbon delta (savings)"
                );
                found_cpu_down = true;
            }
        }
        assert!(found_cpu_down, "Expected CPU scale down recommendation");
    }

    #[test]
    fn test_eco_scale_down_ram() {
        let config = test_config(2, 4096);
        // Moderate CPU (50%), Low RAM (10%)
        let snap = test_snapshot(50.0, 4096 * 1024 * 1024, 409 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);

        let recs = snap.recommend_eco_sizing(&config, &factors);
        assert!(!recs.is_empty());

        let mut found_ram_down = false;
        for rec in &recs {
            if let ResizeRecommendation::ScaleDownRam { suggested_mib, .. } = rec.recommendation {
                assert_eq!(suggested_mib, 2048);
                assert!(
                    rec.carbon_delta_mg_per_sec < 0.0,
                    "Scaling down RAM should result in negative carbon delta (savings)"
                );
                found_ram_down = true;
            }
        }
        assert!(found_ram_down, "Expected RAM scale down recommendation");
    }

    #[test]
    fn test_eco_scale_up_cpu() {
        let config = test_config(2, 2048);
        // High CPU (95%), Moderate RAM (50%)
        let snap = test_snapshot(95.0, 2048 * 1024 * 1024, 1024 * 1024 * 1024);
        let factors = EmissionFactors::new(400.0);

        let recs = snap.recommend_eco_sizing(&config, &factors);
        assert!(!recs.is_empty());

        let mut found_cpu_up = false;
        for rec in &recs {
            if let ResizeRecommendation::ScaleUpCpu { suggested, .. } = rec.recommendation {
                assert_eq!(suggested, 3);
                assert!(
                    rec.carbon_delta_mg_per_sec > 0.0,
                    "Scaling up CPU should result in positive carbon delta (increased emissions)"
                );
                found_cpu_up = true;
            }
        }
        assert!(found_cpu_up, "Expected CPU scale up recommendation");
    }
}
