use comfy_table::{Cell, Color, Row};
use hitz_api::{
    CarbonEstimator, EfficiencyScorer, EmissionFactors, MetricsSnapshot, RightSizer, VmConfig,
};

/// Generates a series of table rows representing the VM's ecological and efficiency report.
pub fn generate_eco_report(
    config: &VmConfig,
    metrics: &MetricsSnapshot,
    grid_intensity: f64,
) -> Vec<Row> {
    let mut rows = Vec::new();

    // 1. Carbon Estimation
    let factors = EmissionFactors::new(grid_intensity);
    let emissions_mg_sec = metrics.estimate_carbon(&factors, config.cpus);

    rows.push(Row::from(vec![
        Cell::new("Carbon Emissions").fg(Color::Green),
        Cell::new(format!("{:.2} mg CO2/sec", emissions_mg_sec)),
        Cell::new(format!("Grid Intensity: {:.0} g/kWh", grid_intensity)),
    ]));

    // 2. Efficiency Score
    let efficiency = metrics.calculate_efficiency();
    let score_color = if efficiency.score >= 80.0 {
        Color::Green
    } else if efficiency.score >= 50.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    let insight_str = if efficiency.insights.is_empty() {
        "Optimal".to_string()
    } else {
        efficiency.insights.join("; ")
    };

    rows.push(Row::from(vec![
        Cell::new("Efficiency Score").fg(score_color),
        Cell::new(format!("{:.1} / 100", efficiency.score)).fg(score_color),
        Cell::new(insight_str),
    ]));

    // 3. Rightsizer Recommendations
    let recs = metrics.recommend_sizing(config);
    if recs.is_empty() {
        rows.push(Row::from(vec![
            Cell::new("Rightsizer").fg(Color::Cyan),
            Cell::new("No changes needed"),
            Cell::new("VM is appropriately sized"),
        ]));
    } else {
        for (i, rec) in recs.iter().enumerate() {
            let label = if i == 0 { "Rightsizer" } else { "" };
            let (action, reason) = match rec {
                hitz_api::ResizeRecommendation::ScaleUpCpu {
                    suggested, reason, ..
                } => (format!("Scale UP to {suggested} vCPUs"), reason),
                hitz_api::ResizeRecommendation::ScaleDownCpu {
                    suggested, reason, ..
                } => (format!("Scale DOWN to {suggested} vCPUs"), reason),
                hitz_api::ResizeRecommendation::ScaleUpRam {
                    suggested_mib,
                    reason,
                    ..
                } => (format!("Scale UP to {suggested_mib} MiB RAM"), reason),
                hitz_api::ResizeRecommendation::ScaleDownRam {
                    suggested_mib,
                    reason,
                    ..
                } => (format!("Scale DOWN to {suggested_mib} MiB RAM"), reason),
            };
            rows.push(Row::from(vec![
                Cell::new(label).fg(Color::Cyan),
                Cell::new(action).fg(Color::Yellow),
                Cell::new(reason),
            ]));
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use hitz_api::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_generate_eco_report_healthy() {
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

        let metrics = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 60.0,
                per_core: vec![60.0, 60.0, 60.0, 60.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 600 * 1024 * 1024,
                free_bytes: 424 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let rows = generate_eco_report(&config, &metrics, 400.0);
        assert_eq!(rows.len(), 3);
        // Carbon row
        assert!(rows[0].cell_iter().any(|c| c.content().contains("Carbon")));
        // Efficiency row
        assert!(
            rows[1]
                .cell_iter()
                .any(|c| c.content().contains("100.0 / 100"))
        );
        // Rightsizer row
        assert!(
            rows[2]
                .cell_iter()
                .any(|c| c.content().contains("No changes needed"))
        );
    }

    #[test]
    fn test_generate_eco_report_inefficient() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048,
            cpus: 8,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let metrics = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 5.0,
                per_core: vec![5.0; 8],
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
        };

        let rows = generate_eco_report(&config, &metrics, 400.0);
        // Should have Carbon, Efficiency, and 2 Rightsizer recommendations
        assert_eq!(rows.len(), 4);
        assert!(
            rows[2]
                .cell_iter()
                .any(|c| c.content().contains("Scale DOWN"))
        );
        assert!(
            rows[3]
                .cell_iter()
                .any(|c| c.content().contains("Scale DOWN"))
        );
    }
}
