//! `FinOps` & Sustainability Module
//!
//! # Abstract
//! This module combines the `CarbonEstimator`, `EfficiencyScorer`, and `RightSizer`
//! traits to provide a comprehensive `FinOps` report for a micro-VM. It quantifies
//! waste in terms of both financial cost (due to underutilization) and carbon emissions.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{FinOpsAnalyzer, EmissionFactors, VmConfig, GuestAgentMode, MetricsSnapshot, CpuMetrics, MemoryMetrics};
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
//! let report = snap.generate_finops_report(&config, &factors);
//! assert!(report.efficiency_score < 100.0);
//! assert!(!report.recommendations.is_empty());
//! ```

use crate::{
    CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot, ResizeRecommendation,
    RightSizer, VmConfig,
};
use serde::{Deserialize, Serialize};

/// A comprehensive `FinOps` and Sustainability report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinOpsReport {
    /// Overall efficiency score (0.0 to 100.0).
    pub efficiency_score: f64,
    /// Estimated carbon emissions in mg CO2/sec.
    pub estimated_carbon_mg_per_sec: f64,
    /// Actionable recommendations to resize the VM.
    pub recommendations: Vec<ResizeRecommendation>,
    /// Human-readable insights regarding efficiency waste.
    pub insights: Vec<String>,
}

/// Trait to generate a `FinOps` report.
pub trait FinOpsAnalyzer {
    /// Generates a comprehensive `FinOps` report based on metrics, config, and emission factors.
    fn generate_finops_report(&self, config: &VmConfig, factors: &EmissionFactors) -> FinOpsReport;
}

impl FinOpsAnalyzer for MetricsSnapshot {
    fn generate_finops_report(&self, config: &VmConfig, factors: &EmissionFactors) -> FinOpsReport {
        let efficiency = self.calculate_efficiency();
        let carbon = self.estimate_carbon(factors, config.cpus);
        let sizing = self.recommend_sizing(config);

        FinOpsReport {
            efficiency_score: efficiency.score,
            estimated_carbon_mg_per_sec: carbon,
            recommendations: sizing,
            insights: efficiency.insights,
        }
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

    fn test_snapshot(cpu_pct: f32, mem_used_mib: u32, ram_mib: u32) -> MetricsSnapshot {
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
    fn test_finops_report_underutilized() {
        let config = test_config(4, 2048);
        let snap = test_snapshot(5.0, 200, 2048); // Underutilized
        let factors = EmissionFactors::new(400.0);

        let report = snap.generate_finops_report(&config, &factors);

        assert!(report.efficiency_score < 50.0);
        assert!(report.estimated_carbon_mg_per_sec > 0.0);
        assert!(!report.recommendations.is_empty());
        assert!(!report.insights.is_empty());
    }

    #[test]
    fn test_finops_report_highly_efficient() {
        let config = test_config(2, 1024);
        let snap = test_snapshot(85.0, 900, 1024); // Efficiently utilized
        let factors = EmissionFactors::new(200.0);

        let report = snap.generate_finops_report(&config, &factors);

        assert!(report.efficiency_score > 90.0);
        assert!(report.estimated_carbon_mg_per_sec > 0.0);
        assert!(report.recommendations.is_empty()); // No scale down recommendations
        assert!(report.insights.is_empty());
    }
}
