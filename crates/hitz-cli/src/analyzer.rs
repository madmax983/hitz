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
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Critical => write!(f, "CRIT"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResourceInsight {
    pub level: WarningLevel,
    pub message: String,
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
        #[allow(clippy::cast_precision_loss)]
        let used_pct =
            (metrics.memory.used_bytes as f64 / metrics.memory.total_bytes as f64) * 100.0;
        if used_pct > 90.0 {
            insights.push(ResourceInsight {
                level: WarningLevel::Critical,
                message: format!("High Memory utilization: {used_pct:.1}% (OOM Risk)"),
            });
        } else if used_pct > 75.0 {
            insights.push(ResourceInsight {
                level: WarningLevel::Warning,
                message: format!("Elevated Memory utilization: {used_pct:.1}%"),
            });
        } else {
            insights.push(ResourceInsight {
                level: WarningLevel::Info,
                message: format!("Healthy Memory utilization: {used_pct:.1}%"),
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
    if let Some(top_proc) = metrics.processes.first().filter(|p| p.cpu_pct > 80.0) {
        insights.push(ResourceInsight {
            level: WarningLevel::Warning,
            message: format!(
                "Process '{}' (PID {}) is consuming {:.1}% CPU",
                top_proc.name, top_proc.pid, top_proc.cpu_pct
            ),
        });
    }

    // Network Errors Analysis
    let rx_errs: u64 = metrics.networks.iter().map(|n| n.rx_errors).sum();
    let tx_errs: u64 = metrics.networks.iter().map(|n| n.tx_errors).sum();

    if rx_errs > 0 || tx_errs > 0 {
        insights.push(ResourceInsight {
            level: WarningLevel::Warning,
            message: format!(
                "Network errors detected (RX: {rx_errs}, TX: {tx_errs})"
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
}
