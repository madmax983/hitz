use crate::analyzer::ResourceInsight;
use hitz_api::{MetricsSnapshot, VmInfo};

pub fn generate_markdown_report(
    info: &VmInfo,
    metrics: &MetricsSnapshot,
    insights: &[ResourceInsight],
) -> String {
    let mut report = String::new();

    report.push_str(&format!("# VM Health Report: {}\n\n", info.id));

    report.push_str(&format!("**State:** {:?}\n\n", info.state));

    report.push_str("## Configuration\n\n");
    report.push_str(&format!("- **CPUs:** {}\n", info.config.cpus));
    report.push_str(&format!("- **RAM:** {} MiB\n", info.config.ram_mib));
    report.push_str(&format!(
        "- **Kernel:** {}\n",
        info.config.kernel_path.display()
    ));

    report.push_str("\n## Current Metrics\n\n");
    report.push_str(&format!("- **CPU Usage:** {:.1}%\n", metrics.cpu.total_pct));

    let used_mib = metrics.memory.used_bytes / (1024 * 1024);
    let total_mib = metrics.memory.total_bytes / (1024 * 1024);
    report.push_str(&format!(
        "- **Memory Usage:** {} MiB / {} MiB\n",
        used_mib, total_mib
    ));

    report.push_str("\n## Health Insights\n\n");
    if insights.is_empty() {
        report.push_str("- No insights available.\n");
    } else {
        for insight in insights {
            report.push_str(&format!("- **[{}]** {}\n", insight.level, insight.message));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::WarningLevel;
    use hitz_api::{
        CpuMetrics, DEFAULT_CPUS, DEFAULT_GUEST_CID, GuestAgentMode, MemoryMetrics, VmConfig,
        VmState,
    };
    use std::path::PathBuf;

    #[test]
    fn test_generate_markdown_report() {
        let info = VmInfo {
            id: "test-vm".to_string(),
            state: VmState::Running,
            config: VmConfig {
                kernel_path: PathBuf::from("vmlinux"),
                initramfs_path: None,
                disk_path: None,
                ram_mib: 1024,
                cpus: DEFAULT_CPUS,
                cmdline: None,
                net: None,
                ports: vec![],
                guest_cid: DEFAULT_GUEST_CID,
                guest_agent: GuestAgentMode::Auto,
            },
            exit_reason: None,
        };
        let metrics = MetricsSnapshot {
            timestamp_ms: 1234567890,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.1, 0.2, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 256 * 1024 * 1024,
                free_bytes: 768 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let insights = vec![ResourceInsight {
            level: WarningLevel::Info,
            message: "All good".to_string(),
        }];

        let report = generate_markdown_report(&info, &metrics, &insights);
        assert!(report.contains("# VM Health Report: test-vm"));
        assert!(report.contains("## Configuration"));
        assert!(report.contains("1024 MiB"));
        assert!(report.contains("## Health Insights"));
        assert!(report.contains("All good"));
    }
}
