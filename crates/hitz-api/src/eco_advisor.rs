//! Carbon-aware rightsizing recommendations.
//!
//! # Abstract
//! This module combines the logic from the `rightsizer` and `carbon` modules
//! to provide `EcoRecommendation`s. It not only suggests how to resize a VM
//! for better efficiency, but also estimates the carbon emissions (gCO2eq)
//! saved (or cost) by applying each recommendation.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EcoAdvisor, EmissionFactors, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
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
//!     cpu: CpuMetrics { total_pct: 5.0, per_core: vec![5.0; 8], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics {
//!         total_bytes: 4096 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 3900 * 1024 * 1024,
//!         buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
//!     },
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let factors = EmissionFactors::new(400.0);
//! let eco_recs = snap.advise_eco(&config, &factors);
//!
//! assert!(!eco_recs.is_empty());
//! // We should see carbon savings for scaling down!
//! assert!(eco_recs[0].carbon_savings_mg_per_sec > 0.0);
//! ```

use crate::{
    CarbonEstimator, EmissionFactors, MetricsSnapshot, ResizeRecommendation, RightSizer, VmConfig,
};
use serde::{Deserialize, Serialize};

/// An ecologically-aware resizing recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoRecommendation {
    /// The underlying sizing recommendation.
    pub resize: ResizeRecommendation,
    /// The estimated change in carbon emissions (in mg CO2/sec).
    /// Positive values indicate carbon *savings* (emissions reduction),
    /// while negative values indicate an *increase* in emissions.
    pub carbon_savings_mg_per_sec: f64,
}

/// Trait for objects that can advise on carbon-aware resizing.
pub trait EcoAdvisor {
    /// Evaluates current metrics and config, returning sizing recommendations
    /// enriched with their estimated carbon impact.
    fn advise_eco(&self, config: &VmConfig, factors: &EmissionFactors) -> Vec<EcoRecommendation>;
}

impl EcoAdvisor for MetricsSnapshot {
    fn advise_eco(&self, config: &VmConfig, factors: &EmissionFactors) -> Vec<EcoRecommendation> {
        let base_recs = self.recommend_sizing(config);
        let mut eco_recs = Vec::with_capacity(base_recs.len());

        let current_emissions = self.estimate_carbon(factors, config.cpus);

        for rec in base_recs {
            let savings = match &rec {
                ResizeRecommendation::ScaleUpCpu { suggested, .. }
                | ResizeRecommendation::ScaleDownCpu { suggested, .. } => {
                    let new_emissions = self.estimate_carbon(factors, *suggested);
                    current_emissions - new_emissions
                }
                ResizeRecommendation::ScaleUpRam { suggested_mib, .. }
                | ResizeRecommendation::ScaleDownRam { suggested_mib, .. } => {
                    let mut hypothetical_snap = self.clone();
                    hypothetical_snap.memory.total_bytes = u64::from(*suggested_mib) * 1024 * 1024;
                    let new_emissions = hypothetical_snap.estimate_carbon(factors, config.cpus);
                    current_emissions - new_emissions
                }
            };

            eco_recs.push(EcoRecommendation {
                resize: rec,
                carbon_savings_mg_per_sec: savings,
            });
        }

        eco_recs
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
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

    fn test_snapshot(cpu_pct: f32, ram_mib: u32, used_mib: u32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: u64::from(ram_mib) * 1024 * 1024,
                used_bytes: u64::from(used_mib) * 1024 * 1024,
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
    fn test_eco_advisor_scale_down() {
        let config = test_config(8, 4096);
        let snap = test_snapshot(5.0, 4096, 100);
        let factors = EmissionFactors::new(400.0);

        let recs = snap.advise_eco(&config, &factors);
        assert_eq!(recs.len(), 2);

        // Scaling down should result in positive carbon savings
        for rec in recs {
            assert!(
                rec.carbon_savings_mg_per_sec > 0.0,
                "Expected positive savings for scale down"
            );
        }
    }

    #[test]
    fn test_eco_advisor_scale_up() {
        let config = test_config(2, 512);
        let snap = test_snapshot(95.0, 512, 500);
        let factors = EmissionFactors::new(400.0);

        let recs = snap.advise_eco(&config, &factors);
        assert_eq!(recs.len(), 2);

        // Scaling up should result in negative carbon savings (cost)
        for rec in recs {
            assert!(
                rec.carbon_savings_mg_per_sec < 0.0,
                "Expected negative savings for scale up"
            );
        }
    }
}
