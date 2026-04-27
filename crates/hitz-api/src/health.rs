//! Health assessment module for evaluating system telemetry.
//!
//! # Abstract
//! This module provides the tools necessary to evaluate the current health
//! of a system based on its telemetry metrics. It defines what it means to be
//! healthy, warns when things are getting hot, and screams when the system is on fire.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{HealthCheck, HealthStatus};
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // We have some metrics from our micro-VM
//! let metrics = MetricsSnapshot {
//!     timestamp_ms: 1_700_000_000_000,
//!     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![95.0], load_avg: [2.0, 1.5, 1.0] },
//!     memory: MemoryMetrics { total_bytes: 100, used_bytes: 10, free_bytes: 90, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // We assess the health of the system
//! let health = metrics.assess_health();
//!
//! // Because the CPU is at 95%, we are in a critical state!
//! assert_eq!(health.status, HealthStatus::Critical);
//! assert!(health.reasons[0].contains("Critical CPU usage"));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The overall health status of the system or component.
///
/// # Abstract
/// An enum representing the three states of being for a micro-VM:
/// perfectly fine, starting to sweat, and actively melting down.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::HealthStatus;
///
/// let my_status = HealthStatus::Healthy;
/// assert_eq!(my_status, HealthStatus::Healthy);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HealthStatus {
    /// System is operating within normal parameters.
    Healthy,
    /// System is experiencing elevated load or minor issues.
    Warning,
    /// System is experiencing severe resource exhaustion or critical errors.
    Critical,
}

/// The result of a health assessment.
///
/// # Abstract
/// A concrete report card for the system. It not only tells you the current
/// [`HealthStatus`], but if things aren't [`HealthStatus::Healthy`], it provides
/// human-readable reasons explaining *why*.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{SystemHealth, HealthStatus};
///
/// let report = SystemHealth {
///     status: HealthStatus::Warning,
///     reasons: vec!["CPU is getting a bit warm".to_string()],
/// };
///
/// assert_eq!(report.status, HealthStatus::Warning);
/// ```
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{SystemHealth, HealthStatus};
///
/// let health = SystemHealth {
///     status: HealthStatus::Warning,
///     reasons: vec!["High CPU usage".to_string()],
/// };
///
/// assert_eq!(health.status, HealthStatus::Warning);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemHealth {
    /// The aggregated health status.
    pub status: HealthStatus,
    /// A list of human-readable reasons explaining the current status.
    /// Empty if `status` is `Healthy`.
    pub reasons: Vec<String>,
}

/// Trait for types that can be evaluated for system health.
///
/// # Abstract
/// Any type that can be examined to determine if the system is healthy
/// should implement this trait. It provides a standard interface for
/// getting a [`SystemHealth`] report.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{HealthCheck, SystemHealth, HealthStatus};
///
/// struct MySystem;
///
/// impl HealthCheck for MySystem {
///     fn assess_health(&self) -> SystemHealth {
///         SystemHealth {
///             status: HealthStatus::Healthy,
///             reasons: vec![],
///         }
///     }
/// }
///
/// let sys = MySystem;
/// let report = sys.assess_health();
/// assert_eq!(report.status, HealthStatus::Healthy);
/// ```
pub trait HealthCheck {
    /// Assesses the health of the entity and returns a `SystemHealth` report.
    fn assess_health(&self) -> SystemHealth;
}

impl HealthCheck for MetricsSnapshot {
    /// Assesses the health of the micro-VM based on its metrics.
    ///
    /// # Abstract
    /// Evaluates the CPU, memory, swap, and network metrics to determine the
    /// overall health of the system.
    ///
    /// # Details
    /// The evaluation is based on thresholds:
    /// - **CPU**: > 90% is Critical, > 75% is Warning
    /// - **Memory**: > 90% is Critical, > 75% is Warning
    /// - **Swap**: > 50% is Critical, > 20% is Warning
    /// - **Network Errors**: > 100 is Critical, > 10 is Warning
    #[allow(clippy::cast_precision_loss)]
    fn assess_health(&self) -> SystemHealth {
        let mut status = HealthStatus::Healthy;
        // ⚡ Bolt Optimization:
        // Pre-allocate the `reasons` vector to its maximum possible size (4)
        // to avoid any heap reallocations during health assessment, as there
        // are exactly 4 evaluation checks (CPU, Memory, Swap, Network).
        let mut reasons = Vec::with_capacity(4);

        // CPU evaluation
        if self.cpu.total_pct > 90.0 {
            status = status.max(HealthStatus::Critical);
            reasons.push(format!("Critical CPU usage: {:.1}%", self.cpu.total_pct));
        } else if self.cpu.total_pct > 75.0 {
            status = status.max(HealthStatus::Warning);
            reasons.push(format!("High CPU usage: {:.1}%", self.cpu.total_pct));
        }

        // Memory evaluation
        if self.memory.total_bytes > 0 {
            let memory_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            if memory_pct > 90.0 {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical memory usage: {memory_pct:.1}%"));
            } else if memory_pct > 75.0 {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("High memory usage: {memory_pct:.1}%"));
            }
        }

        // Swap evaluation
        if self.memory.swap_total > 0 {
            #[allow(clippy::cast_precision_loss)]
            let swap_pct = (self.memory.swap_used as f64 / self.memory.swap_total as f64) * 100.0;
            if swap_pct > 50.0 {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical swap usage: {swap_pct:.1}%"));
            } else if swap_pct > 20.0 {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("High swap usage: {swap_pct:.1}%"));
            }
        }

        // Network errors evaluation
        let total_net_errors: u64 = self
            .networks
            .iter()
            .map(|net| net.rx_errors + net.tx_errors)
            .sum();

        if total_net_errors > 100 {
            status = status.max(HealthStatus::Critical);
            reasons.push(format!("Critical network errors: {total_net_errors}"));
        } else if total_net_errors > 10 {
            status = status.max(HealthStatus::Warning);
            reasons.push(format!("Elevated network errors: {total_net_errors}"));
        }

        SystemHealth { status, reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, NetMetrics};

    fn safe_metrics() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 100,
                free_bytes: 900,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 1000,
                swap_used: 100,
            },
            disks: vec![],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        }
    }

    #[test]
    fn should_return_healthy_when_metrics_are_low() {
        let metrics = safe_metrics();
        let health = metrics.assess_health();
        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.reasons.is_empty());
    }

    #[test]
    fn should_return_warning_when_cpu_is_elevated() {
        let mut metrics = safe_metrics();
        metrics.cpu.total_pct = 80.0;
        let health = metrics.assess_health();
        assert_eq!(health.status, HealthStatus::Warning);
        assert_eq!(health.reasons.len(), 1);
        assert!(health.reasons[0].contains("High CPU"));
    }

    #[test]
    fn should_return_critical_when_cpu_exhausted() {
        let mut metrics = safe_metrics();
        metrics.cpu.total_pct = 95.0; // > 90.0 triggers Critical
        let health = metrics.assess_health();
        assert_eq!(health.status, HealthStatus::Critical);
        assert_eq!(health.reasons.len(), 1);
        assert!(health.reasons[0].contains("Critical CPU"));
    }

    #[test]
    fn should_evaluate_memory_and_swap_thresholds_correctly() {
        struct TestCase {
            mem_total: u64,
            mem_used: u64,
            swap_total: u64,
            swap_used: u64,
            expected: HealthStatus,
            has_mem_reason: bool,
            has_swap_reason: bool,
        }

        let test_cases = vec![
            TestCase {
                mem_total: 1000,
                mem_used: 100,
                swap_total: 1000,
                swap_used: 100,
                expected: HealthStatus::Healthy,
                has_mem_reason: false,
                has_swap_reason: false,
            },
            TestCase {
                // Warning memory (>75%)
                mem_total: 1000,
                mem_used: 800,
                swap_total: 1000,
                swap_used: 100,
                expected: HealthStatus::Warning,
                has_mem_reason: true,
                has_swap_reason: false,
            },
            TestCase {
                // Critical memory (>90%)
                mem_total: 1000,
                mem_used: 950,
                swap_total: 1000,
                swap_used: 100,
                expected: HealthStatus::Critical,
                has_mem_reason: true,
                has_swap_reason: false,
            },
            TestCase {
                // Warning swap (>20%)
                mem_total: 1000,
                mem_used: 100,
                swap_total: 1000,
                swap_used: 300,
                expected: HealthStatus::Warning,
                has_mem_reason: false,
                has_swap_reason: true,
            },
            TestCase {
                // Critical swap (>50%)
                mem_total: 1000,
                mem_used: 100,
                swap_total: 1000,
                swap_used: 600,
                expected: HealthStatus::Critical,
                has_mem_reason: false,
                has_swap_reason: true,
            },
            TestCase {
                // 0 byte total avoids division by zero panic and keeps Healthy
                mem_total: 0,
                mem_used: 0,
                swap_total: 0,
                swap_used: 0,
                expected: HealthStatus::Healthy,
                has_mem_reason: false,
                has_swap_reason: false,
            },
        ];

        for case in test_cases {
            let mut metrics = safe_metrics();
            metrics.memory.total_bytes = case.mem_total;
            metrics.memory.used_bytes = case.mem_used;
            metrics.memory.swap_total = case.swap_total;
            metrics.memory.swap_used = case.swap_used;

            let health = metrics.assess_health();
            assert_eq!(
                health.status, case.expected,
                "Failed for mem_used={} swap_used={}",
                case.mem_used, case.swap_used
            );

            if case.has_mem_reason {
                assert!(health.reasons.iter().any(|r| r.contains("memory")));
            }
            if case.has_swap_reason {
                assert!(health.reasons.iter().any(|r| r.contains("swap")));
            }
        }
    }

    #[test]
    fn should_evaluate_network_errors_correctly() {
        struct TestCase {
            rx_errs: u64,
            tx_errs: u64,
            expected: HealthStatus,
        }

        let test_cases = vec![
            TestCase {
                rx_errs: 5,
                tx_errs: 5,
                expected: HealthStatus::Healthy,
            },
            TestCase {
                rx_errs: 10,
                tx_errs: 1,
                expected: HealthStatus::Warning,
            },
            TestCase {
                rx_errs: 50,
                tx_errs: 50,
                expected: HealthStatus::Warning,
            },
            TestCase {
                rx_errs: 100,
                tx_errs: 1,
                expected: HealthStatus::Critical,
            },
            TestCase {
                rx_errs: 50,
                tx_errs: 60,
                expected: HealthStatus::Critical,
            },
        ];

        for case in test_cases {
            let mut metrics = safe_metrics();
            metrics.networks[0].rx_errors = case.rx_errs;
            metrics.networks[0].tx_errors = case.tx_errs;
            let health = metrics.assess_health();
            assert_eq!(
                health.status, case.expected,
                "Failed for rx_errors={} tx_errors={}",
                case.rx_errs, case.tx_errs
            );
            if case.expected != HealthStatus::Healthy {
                assert!(!health.reasons.is_empty());
                assert!(health.reasons.iter().any(|r| r.contains("network errors")));
            }
        }
    }
}
