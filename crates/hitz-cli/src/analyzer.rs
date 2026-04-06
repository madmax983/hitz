use hitz_api::{MetricsSnapshot, VmInfo};

#[derive(Debug, PartialEq, Eq)]
pub enum WarningLevel {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for WarningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningLevel::Info => write!(f, "INFO"),
            WarningLevel::Warning => write!(f, "WARN"),
            WarningLevel::Critical => write!(f, "CRIT"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResourceInsight {
    pub level: WarningLevel,
    pub message: String,
}

pub struct TrendInsight {
    pub level: WarningLevel,
    pub message: String,
}

pub fn analyze_record_history(snapshots: &[MetricsSnapshot]) -> Vec<TrendInsight> {
    let mut insights = Vec::new();

    if snapshots.len() < 2 {
        insights.push(TrendInsight {
            level: WarningLevel::Info,
            message: "Not enough data points for trend analysis.".into(),
        });
        return insights;
    }

    let first = &snapshots[0];
    let last = snapshots.last().unwrap();
    let duration_ms = last.timestamp_ms.saturating_sub(first.timestamp_ms);

    if duration_ms < 5000 {
        insights.push(TrendInsight {
            level: WarningLevel::Info,
            message: "Recording duration is too short (< 5s) for meaningful long-term trend analysis.".into(),
        });
        // We still continue to see if any immediate spikes occurred, but trends might be noisy
    }

    // --- CPU Starvation Analysis ---
    // Count how many consecutive snapshots had high CPU
    let mut high_cpu_streak = 0;
    let mut max_high_cpu_streak = 0;

    for snap in snapshots {
        if snap.cpu.total_pct > 90.0 {
            high_cpu_streak += 1;
            if high_cpu_streak > max_high_cpu_streak {
                max_high_cpu_streak = high_cpu_streak;
            }
        } else {
            high_cpu_streak = 0;
        }
    }

    // Assuming 1 snapshot per second, 10 consecutive means ~10 seconds of >90% CPU
    if max_high_cpu_streak > 10 {
        insights.push(TrendInsight {
            level: WarningLevel::Critical,
            message: format!("Prolonged CPU Starvation detected: CPU > 90% for {} consecutive samples.", max_high_cpu_streak),
        });
    }

    // --- Memory Leak Detection ---
    let first_mem = first.memory.used_bytes;
    let last_mem = last.memory.used_bytes;

    if last_mem > first_mem {
        let diff = last_mem - first_mem;
        let diff_mb = diff as f64 / (1024.0 * 1024.0);

        // Simple heuristic: if memory monotonically increased across the *entire* trace
        // and grew by more than 10MB, warn about potential leak.
        let mut monotonically_increasing = true;
        for window in snapshots.windows(2) {
            if window[1].memory.used_bytes < window[0].memory.used_bytes {
                monotonically_increasing = false;
                break;
            }
        }

        if monotonically_increasing && diff_mb > 10.0 {
            insights.push(TrendInsight {
                level: WarningLevel::Critical,
                message: format!("Potential Memory Leak: Used memory increased monotonically by {:.1} MB over the recording.", diff_mb),
            });
        } else if diff_mb > 50.0 {
            // Even if not monotonically increasing, a huge jump is worth noting
             insights.push(TrendInsight {
                level: WarningLevel::Warning,
                message: format!("Significant Memory Growth: Used memory is {:.1} MB higher at end than at start.", diff_mb),
            });
        }
    }

    // --- Network Error Spikes ---
    let initial_rx_errs: u64 = first.networks.iter().map(|n| n.rx_errors).sum();
    let initial_tx_errs: u64 = first.networks.iter().map(|n| n.tx_errors).sum();
    let final_rx_errs: u64 = last.networks.iter().map(|n| n.rx_errors).sum();
    let final_tx_errs: u64 = last.networks.iter().map(|n| n.tx_errors).sum();

    if final_rx_errs > initial_rx_errs {
        insights.push(TrendInsight {
            level: WarningLevel::Warning,
            message: format!("Network RX Errors increased by {} during the recording.", final_rx_errs - initial_rx_errs),
        });
    }

    if final_tx_errs > initial_tx_errs {
        insights.push(TrendInsight {
            level: WarningLevel::Warning,
            message: format!("Network TX Errors increased by {} during the recording.", final_tx_errs - initial_tx_errs),
        });
    }

    insights
}

pub fn analyze_vm(info: &VmInfo, metrics: &MetricsSnapshot) -> Vec<ResourceInsight> {
    let mut insights = Vec::new();

    // Analyze CPU usage
    if metrics.cpu.total_pct > 90.0 {
        insights.push(ResourceInsight {
            level: WarningLevel::Critical,
            message: format!("High CPU utilization: {:.1}%", metrics.cpu.total_pct),
        });
    } else if metrics.cpu.total_pct > 75.0 {
        insights.push(ResourceInsight {
            level: WarningLevel::Warning,
            message: format!("Elevated CPU utilization: {:.1}%", metrics.cpu.total_pct),
        });
    } else {
        insights.push(ResourceInsight {
            level: WarningLevel::Info,
            message: format!("Healthy CPU utilization: {:.1}%", metrics.cpu.total_pct),
        });
    }

    // Analyze Memory usage
    // Avoid division by zero
    if metrics.memory.total_bytes > 0 {
        // According to hitz_api, buffers and cached bytes are often reclaimable, but for a simple score,
        // let's look at used_bytes vs total_bytes.
        let used_pct =
            (metrics.memory.used_bytes as f64 / metrics.memory.total_bytes as f64) * 100.0;
        if used_pct > 90.0 {
            insights.push(ResourceInsight {
                level: WarningLevel::Critical,
                message: format!("High Memory utilization: {:.1}% (OOM Risk)", used_pct),
            });
        } else if used_pct > 75.0 {
            insights.push(ResourceInsight {
                level: WarningLevel::Warning,
                message: format!("Elevated Memory utilization: {:.1}%", used_pct),
            });
        } else {
            insights.push(ResourceInsight {
                level: WarningLevel::Info,
                message: format!("Healthy Memory utilization: {:.1}%", used_pct),
            });
        }

        // Over-provisioning check: if VM is using very little RAM but configured with a lot.
        if used_pct < 10.0 && info.config.ram_mib > 512 {
            insights.push(ResourceInsight {
                level: WarningLevel::Info,
                message: "VM may be over-provisioned for memory (using <10% of >512 MiB)"
                    .to_string(),
            });
        }
    }

    // Process analysis
    if let Some(top_proc) = metrics.processes.first() {
        if top_proc.cpu_pct > 80.0 {
            insights.push(ResourceInsight {
                level: WarningLevel::Warning,
                message: format!(
                    "Process '{}' (PID {}) is consuming {:.1}% CPU",
                    top_proc.name, top_proc.pid, top_proc.cpu_pct
                ),
            });
        }
    }

    // Network Errors Analysis
    let total_rx_errs: u64 = metrics.networks.iter().map(|n| n.rx_errors).sum();
    let total_tx_errs: u64 = metrics.networks.iter().map(|n| n.tx_errors).sum();

    if total_rx_errs > 0 || total_tx_errs > 0 {
        insights.push(ResourceInsight {
            level: WarningLevel::Warning,
            message: format!(
                "Network errors detected (RX: {}, TX: {})",
                total_rx_errs, total_tx_errs
            ),
        });
    }

    insights
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use hitz_api::{
        CpuMetrics, DEFAULT_CPUS, DEFAULT_GUEST_CID, GuestAgentMode, MemoryMetrics, VmConfig,
        VmState,
    };
    use std::path::PathBuf;

    fn dummy_info() -> VmInfo {
        VmInfo {
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
        }
    }

    fn dummy_metrics() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024, // 1 GiB
                used_bytes: 256 * 1024 * 1024,   // 256 MiB
                free_bytes: 768 * 1024 * 1024,
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
    fn test_healthy_vm() {
        let info = dummy_info();
        let metrics = dummy_metrics();
        let insights = analyze_vm(&info, &metrics);

        assert!(
            insights
                .iter()
                .any(|i| i.level == WarningLevel::Info && i.message.contains("Healthy CPU"))
        );
        assert!(
            insights
                .iter()
                .any(|i| i.level == WarningLevel::Info && i.message.contains("Healthy Memory"))
        );
        assert!(
            !insights
                .iter()
                .any(|i| i.level == WarningLevel::Warning || i.level == WarningLevel::Critical)
        );
    }

    #[test]
    fn test_high_cpu_warning() {
        let info = dummy_info();
        let mut metrics = dummy_metrics();
        metrics.cpu.total_pct = 95.0;
        let insights = analyze_vm(&info, &metrics);

        assert!(
            insights
                .iter()
                .any(|i| i.level == WarningLevel::Critical && i.message.contains("High CPU"))
        );
    }

    #[test]
    fn test_high_memory_warning() {
        let info = dummy_info();
        let mut metrics = dummy_metrics();
        metrics.memory.used_bytes = 950 * 1024 * 1024; // >90% of 1 GiB
        let insights = analyze_vm(&info, &metrics);

        assert!(
            insights
                .iter()
                .any(|i| i.level == WarningLevel::Critical && i.message.contains("High Memory"))
        );
    }

    #[test]
    fn test_over_provisioned_memory() {
        let info = dummy_info();
        let mut metrics = dummy_metrics();
        metrics.memory.used_bytes = 50 * 1024 * 1024; // <10% of 1 GiB
        let insights = analyze_vm(&info, &metrics);

        assert!(
            insights
                .iter()
                .any(|i| i.level == WarningLevel::Info && i.message.contains("over-provisioned"))
        );
    }

    #[test]
    fn test_analyze_record_history_short_duration() {
        let mut m1 = dummy_metrics();
        m1.timestamp_ms = 0;
        let mut m2 = dummy_metrics();
        m2.timestamp_ms = 1000;

        let insights = analyze_record_history(&[m1, m2]);
        assert!(
            insights.iter().any(|i| i.message.contains("duration is too short"))
        );
    }

    #[test]
    fn test_analyze_record_history_cpu_starvation() {
        let mut snaps = Vec::new();
        for i in 0..15 {
            let mut m = dummy_metrics();
            m.timestamp_ms = i * 1000;
            m.cpu.total_pct = 95.0; // consistently high
            snaps.push(m);
        }

        let insights = analyze_record_history(&snaps);
        assert!(
            insights.iter().any(|i| i.level == WarningLevel::Critical && i.message.contains("CPU Starvation"))
        );
    }

    #[test]
    fn test_analyze_record_history_memory_leak() {
        let mut snaps = Vec::new();
        let mut used_mem = 256 * 1024 * 1024;
        for i in 0..10 {
            let mut m = dummy_metrics();
            m.timestamp_ms = i * 1000;
            m.memory.used_bytes = used_mem;
            snaps.push(m);
            used_mem += 2 * 1024 * 1024; // increase by 2MB each time
        }

        let insights = analyze_record_history(&snaps);
        // Total diff is ~18MB, which is > 10MB threshold for monotonically increasing
        assert!(
            insights.iter().any(|i| i.level == WarningLevel::Critical && i.message.contains("Potential Memory Leak"))
        );
    }
}
