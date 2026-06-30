//! Markdown Telemetry Reporter.
//!
//! # Abstract
//! Generates a comprehensive markdown report by combining various telemetry
//! and analysis modules (efficiency, imbalance, carbon, rightsizer).

#[cfg(any(
    feature = "carbon",
    feature = "efficiency",
    feature = "imbalance",
    feature = "rightsizer"
))]
use crate::{MetricsSnapshot, VmConfig};

#[cfg(feature = "carbon")]
use crate::carbon::{CarbonEstimator, EmissionFactors};

#[cfg(feature = "efficiency")]
use crate::efficiency::EfficiencyScorer;

#[cfg(feature = "imbalance")]
use crate::imbalance::CoreImbalanceAnalyzer;

#[cfg(feature = "rightsizer")]
use crate::rightsizer::RightSizer;

use std::fmt::Write as _;

#[cfg(all(
    feature = "carbon",
    feature = "efficiency",
    feature = "imbalance",
    feature = "rightsizer"
))]
/// Trait to generate a markdown report from metrics.
pub trait MarkdownReporter {
    /// Generates a unified markdown string for the VM's current state.
    fn generate_markdown_report(&self, config: &VmConfig, factors: &EmissionFactors) -> String;
}

#[cfg(all(
    feature = "carbon",
    feature = "efficiency",
    feature = "imbalance",
    feature = "rightsizer"
))]
impl MarkdownReporter for MetricsSnapshot {
    fn generate_markdown_report(&self, config: &VmConfig, factors: &EmissionFactors) -> String {
        let mut report = String::new();

        let _ = writeln!(report, "# VM Telemetry Report");
        let _ = writeln!(report, "Timestamp: {} ms\n", self.timestamp_ms);

        // Carbon
        let emissions = self.estimate_carbon(factors, config.cpus);
        let _ = writeln!(report, "## Carbon Footprint");
        let _ = writeln!(
            report,
            "- Current Emission Rate: {emissions:.2} mg CO2/sec\n"
        );

        // Efficiency
        let eff = self.calculate_efficiency();
        let _ = writeln!(report, "## Efficiency");
        let _ = writeln!(report, "- Score: {:.2}/100", eff.score);
        for insight in eff.insights {
            let _ = writeln!(report, "- Insight: {insight}");
        }
        let _ = writeln!(report);

        // Imbalance
        let imb = self.analyze_imbalance();
        let _ = writeln!(report, "## Core Imbalance");
        let _ = writeln!(report, "- Imbalanced: {}", imb.is_imbalanced);
        let _ = writeln!(report, "- Score: {:.2}\n", imb.imbalance_score);

        // Rightsizing
        let recs = self.recommend_sizing(config);
        let _ = writeln!(report, "## Rightsizing Recommendations");
        if recs.is_empty() {
            let _ = writeln!(report, "- System is optimally sized.");
        } else {
            for r in recs {
                let _ = writeln!(report, "- {:?}", r);
            }
        }

        report
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "carbon",
    feature = "efficiency",
    feature = "imbalance",
    feature = "rightsizer"
))]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, config::GuestAgentMode};
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

        let factors = EmissionFactors::new(400.0);

        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 15.0,
                per_core: vec![15.0; 4],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 200 * 1024 * 1024,
                free_bytes: 800 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let report = snap.generate_markdown_report(&config, &factors);
        assert!(report.contains("# VM Telemetry Report"));
        assert!(report.contains("## Efficiency"));
        assert!(report.contains("## Carbon Footprint"));
    }
}
