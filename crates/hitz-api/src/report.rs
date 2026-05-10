//! Markdown Reporting module for VM telemetry.
//!
//! # Abstract
//! This module combines health, efficiency, rightsizing, and carbon footprint
//! insights into a unified, human-readable Markdown report.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode, EmissionFactors};
//! use hitz_api::report::MarkdownReport;
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
//! let factors = EmissionFactors::new(400.0);
//! let report = snap.generate_markdown_report(&config, &factors);
//! assert!(report.contains("# VM Telemetry Report"));
//! ```

use crate::{
    CarbonEstimator, EfficiencyScorer, EmissionFactors, HealthCheck, MetricsSnapshot, RightSizer,
    VmConfig,
};
use std::fmt::Write as _;

/// Trait to generate a unified Markdown report from telemetry.
pub trait MarkdownReport {
    /// Generates a comprehensive Markdown report including Health, Efficiency,
    /// Rightsizing Recommendations, and Carbon Emissions.
    fn generate_markdown_report(&self, config: &VmConfig, factors: &EmissionFactors) -> String;
}

impl MarkdownReport for MetricsSnapshot {
    fn generate_markdown_report(&self, config: &VmConfig, factors: &EmissionFactors) -> String {
        let mut report = String::new();

        // 1. Header
        let _ = writeln!(report, "# VM Telemetry Report");
        let _ = writeln!(report, "**Timestamp:** {} ms\n", self.timestamp_ms);

        // 2. Health
        let health = self.assess_health();
        let _ = writeln!(report, "## Health Status");
        let _ = writeln!(report, "**Status:** {:?}", health.status);
        if health.reasons.is_empty() {
            let _ = writeln!(
                report,
                "System is running optimally with no critical issues."
            );
        } else {
            for reason in &health.reasons {
                let _ = writeln!(report, "- {reason}");
            }
        }
        let _ = writeln!(report);

        // 3. Efficiency
        let efficiency = self.calculate_efficiency();
        let _ = writeln!(report, "## Resource Efficiency");
        let _ = writeln!(report, "**Score:** {:.1}/100.0", efficiency.score);
        if !efficiency.insights.is_empty() {
            for insight in &efficiency.insights {
                let _ = writeln!(report, "- {insight}");
            }
        }
        let _ = writeln!(report);

        // 4. Rightsizing Recommendations
        let _ = writeln!(report, "## Scaling Recommendations");
        let recommendations = self.recommend_sizing(config);
        if recommendations.is_empty() {
            let _ = writeln!(report, "No configuration changes recommended at this time.");
        } else {
            for rec in &recommendations {
                match rec {
                    crate::ResizeRecommendation::ScaleUpCpu {
                        current,
                        suggested,
                        reason,
                    } => {
                        let _ = writeln!(
                            report,
                            "- **Scale Up CPU:** {current} -> {suggested} ({reason})"
                        );
                    }
                    crate::ResizeRecommendation::ScaleDownCpu {
                        current,
                        suggested,
                        reason,
                    } => {
                        let _ = writeln!(
                            report,
                            "- **Scale Down CPU:** {current} -> {suggested} ({reason})"
                        );
                    }
                    crate::ResizeRecommendation::ScaleUpRam {
                        current_mib,
                        suggested_mib,
                        reason,
                    } => {
                        let _ = writeln!(
                            report,
                            "- **Scale Up RAM:** {current_mib} MiB -> {suggested_mib} MiB ({reason})"
                        );
                    }
                    crate::ResizeRecommendation::ScaleDownRam {
                        current_mib,
                        suggested_mib,
                        reason,
                    } => {
                        let _ = writeln!(
                            report,
                            "- **Scale Down RAM:** {current_mib} MiB -> {suggested_mib} MiB ({reason})"
                        );
                    }
                }
            }
        }
        let _ = writeln!(report);

        // 5. Carbon Emissions
        let _ = writeln!(report, "## Carbon Footprint");
        let emissions = self.estimate_carbon(factors, config.cpus);
        let _ = writeln!(
            report,
            "Estimated Emission Rate: **{emissions:.2} mg CO2eq/sec**"
        );

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_markdown_report_generation() {
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
                total_pct: 5.0,
                per_core: vec![5.0; 4],
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
        let report = snap.generate_markdown_report(&config, &factors);

        assert!(report.contains("# VM Telemetry Report"));
        assert!(report.contains("## Health Status"));
        assert!(report.contains("## Resource Efficiency"));
        assert!(report.contains("## Scaling Recommendations"));
        assert!(report.contains("## Carbon Footprint"));
        assert!(report.contains("Scale Down CPU"));
        assert!(report.contains("Scale Down RAM"));
    }
}
