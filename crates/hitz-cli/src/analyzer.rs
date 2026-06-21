//! Resource Analysis Engine for hitz VMs.
//!
//! # Abstract
//! This module interprets raw telemetry data ([`MetricsSnapshot`]) against a VM's configuration ([`VmInfo`])
//! to generate human-readable health assessments.
//!
//! # The Hero's Journey
//! ```rust
//! # use hitz_cli::analyzer::{analyze_vm, WarningLevel};
//! # use hitz_api::{VmInfo, VmState, VmConfig, GuestAgentMode, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//! # use std::path::PathBuf;
//! #
//! # let info = VmInfo {
//! #     id: "test".to_string(),
//! #     state: VmState::Running,
//! #     config: VmConfig {
//! #         kernel_path: PathBuf::from("vmlinux"),
//! #         initramfs_path: None,
//! #         disk_path: None,
//! #         ram_mib: 1024,
//! #         cpus: 1,
//! #         cmdline: None,
//! #         net: None,
//! #         ports: vec![],
//! #         guest_cid: 3,
//! #         guest_agent: GuestAgentMode::Auto,
//! #     },
//! #     exit_reason: None,
//! # };
//! #
//! # let mut metrics = MetricsSnapshot {
//! #     timestamp_ms: 0,
//! #     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![], load_avg: [0.0; 3] },
//! #     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//! #     disks: vec![],
//! #     networks: vec![],
//! #     processes: vec![],
//! # };
//! #
//! let insights = analyze_vm(&info, &metrics);
//! assert_eq!(insights[0].level, WarningLevel::Critical);
//! assert!(insights[0].message.contains("High CPU utilization"));
//! ```

use hitz_api::{MetricsSnapshot, VmInfo};

/// Severity level of an analysis insight.
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

/// A single observation regarding the VM's health or configuration.
#[derive(Debug, PartialEq, Eq)]
pub struct ResourceInsight {
    /// The severity of the observation.
    pub level: WarningLevel,
    /// A human-readable description of the finding.
    pub message: String,
}

/// Analyzes telemetry data and correlates it with the VM's configuration.
///
/// # Abstract
/// Generates a list of [`ResourceInsight`]s evaluating CPU, Memory, process behavior, and network health.
/// Detects issues like high resource usage, process spinning, and memory over-provisioning.
///
/// ## Examples
/// ```rust
/// # use hitz_cli::analyzer::{analyze_vm, WarningLevel};
/// # use hitz_api::{VmInfo, VmState, VmConfig, GuestAgentMode, MetricsSnapshot, CpuMetrics, MemoryMetrics};
/// # use std::path::PathBuf;
/// #
/// # let info = VmInfo {
/// #     id: "test".to_string(),
/// #     state: VmState::Running,
/// #     config: VmConfig {
/// #         kernel_path: PathBuf::from("vmlinux"),
/// #         initramfs_path: None,
/// #         disk_path: None,
/// #         ram_mib: 2048,
/// #         cpus: 1,
/// #         cmdline: None,
/// #         net: None,
/// #         ports: vec![],
/// #         guest_cid: 3,
/// #         guest_agent: GuestAgentMode::Auto,
/// #     },
/// #     exit_reason: None,
/// # };
/// #
/// # let mut metrics = MetricsSnapshot {
/// #     timestamp_ms: 0,
/// #     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![], load_avg: [0.0; 3] },
/// #     memory: MemoryMetrics { total_bytes: 2048 * 1024 * 1024, used_bytes: 50 * 1024 * 1024, free_bytes: 2000 * 1024 * 1024, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
/// #     disks: vec![],
/// #     networks: vec![],
/// #     processes: vec![],
/// # };
/// #
/// let insights = analyze_vm(&info, &metrics);
///
/// // The memory is drastically underutilized (< 10% of 2048 MiB)
/// assert!(insights.iter().any(|i| i.message.contains("over-provisioned")));
/// ```
pub fn analyze_vm(info: &VmInfo, metrics: &MetricsSnapshot) -> Vec<ResourceInsight> {
    // ⚡ Bolt Optimization: Pre-allocate capacity for up to 4 potential insights
    // (CPU, Memory, Swap, Network) to avoid dynamic heap reallocations.
    let mut insights = Vec::with_capacity(4);

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
            message: format!("Network errors detected (RX: {rx_errs}, TX: {tx_errs})"),
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
