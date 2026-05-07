//! Sustainability Advisor Module.
//!
//! # Abstract
//! This module mashes up the `RightSizer` and `CarbonEstimator` features.
//! It takes resource usage recommendations and calculates the estimated carbon
//! emissions delta (savings or additional cost) if those recommendations are followed.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use hitz_api::EmissionFactors;
//! use hitz_api::sustainability::{SustainabilityAdvisor, EcoRecommendation};
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
//!     cpu: CpuMetrics { total_pct: 5.0, per_core: vec![5.0; 4], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024, used_bytes: 100 * 1024 * 1024, free_bytes: 900 * 1024 * 1024,
//!         buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
//!     },
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let factors = EmissionFactors::new(200.0);
//! let recommendations = snap.advise_sustainability(&config, &factors);
//! assert!(!recommendations.is_empty());
//! ```

use crate::carbon::{CarbonEstimator, EmissionFactors};
use crate::rightsizer::{ResizeRecommendation, RightSizer};
use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Specific actionable recommendation for resizing a VM, bundled with the carbon impact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoRecommendation {
    /// The underlying resize recommendation.
    pub resize: ResizeRecommendation,
    /// Estimated change in carbon emissions (mg CO2/sec).
    /// Negative means a reduction (savings).
    pub carbon_delta_mg_per_sec: f64,
}

/// Trait to generate eco-friendly resizing recommendations.
pub trait SustainabilityAdvisor {
    /// Evaluates metrics to provide sizing recommendations along with their carbon impact.
    fn advise_sustainability(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> Vec<EcoRecommendation>;
}

impl SustainabilityAdvisor for MetricsSnapshot {
    fn advise_sustainability(
        &self,
        config: &VmConfig,
        factors: &EmissionFactors,
    ) -> Vec<EcoRecommendation> {
        let resize_recs = self.recommend_sizing(config);
        let mut eco_recs = Vec::with_capacity(resize_recs.len());

        let current_emissions = self.estimate_carbon(factors, config.cpus);

        for rec in resize_recs {
            let carbon_delta = match rec {
                ResizeRecommendation::ScaleUpCpu { suggested, .. }
                | ResizeRecommendation::ScaleDownCpu { suggested, .. } => {
                    let projected_emissions = self.estimate_carbon(factors, suggested);
                    projected_emissions - current_emissions
                }
                ResizeRecommendation::ScaleUpRam { suggested_mib, .. }
                | ResizeRecommendation::ScaleDownRam { suggested_mib, .. } => {
                    // For RAM, the estimate_carbon implementation currently bases its
                    // calculation on self.memory.total_bytes. So we construct a hypothetical
                    // snapshot with the new RAM total to calculate the delta.
                    let mut hyp_snap = self.clone();
                    hyp_snap.memory.total_bytes = u64::from(suggested_mib) * 1024 * 1024;
                    let projected_emissions = hyp_snap.estimate_carbon(factors, config.cpus);
                    projected_emissions - current_emissions
                }
            };

            eco_recs.push(EcoRecommendation {
                resize: rec,
                carbon_delta_mg_per_sec: carbon_delta,
            });
        }

        eco_recs
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
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
                free_bytes: u64::from(ram_mib.saturating_sub(used_mib)) * 1024 * 1024,
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
    fn test_advise_sustainability_scale_down() {
        let config = test_config(4, 2048);
        let snap = test_snapshot(5.0, 2048, 200); // Low CPU, Low RAM
        let factors = EmissionFactors::new(400.0);

        let recs = snap.advise_sustainability(&config, &factors);
        assert!(!recs.is_empty());

        for rec in recs {
            // Scaling down should result in a negative carbon delta (savings)
            assert!(rec.carbon_delta_mg_per_sec < 0.0);
        }
    }

    #[test]
    fn test_advise_sustainability_scale_up() {
        let config = test_config(2, 512);
        let snap = test_snapshot(95.0, 512, 500); // High CPU, High RAM
        let factors = EmissionFactors::new(400.0);

        let recs = snap.advise_sustainability(&config, &factors);
        assert!(!recs.is_empty());

        for rec in recs {
            // Scaling up should result in a positive carbon delta (extra cost)
            assert!(rec.carbon_delta_mg_per_sec > 0.0);
        }
    }
}
