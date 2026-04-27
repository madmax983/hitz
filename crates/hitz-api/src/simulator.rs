//! Simulator module for generating synthetic telemetry streams.
//!
//! # Abstract
//! This module provides a synthetic [`VmSimulator`] that produces realistic,
//! time-series telemetry data without requiring an actual micro-VM to be running.
//! It is useful for testing, continuous integration, and validating alerting
//! thresholds (e.g., via the [`health`](crate::health) module).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{VmSimulator, WorkloadProfile};
//!
//! // 1. Create a new simulator designed to trigger a CPU alert
//! let mut simulator = VmSimulator::new(WorkloadProfile::CpuSpike);
//!
//! // 2. Consume metrics over time
//! for _ in 0..5 {
//!     let metrics = simulator.next().expect("Simulator never ends");
//!     println!("Current CPU: {}%", metrics.cpu.total_pct);
//! }
//! ```
//!
//! # The Fine Print
//! This simulator implements [`Iterator`] and will run indefinitely.
//! Ensure you place limits on consumption if running in a bounded test environment.

use crate::{CpuMetrics, MemoryMetrics, MetricsSnapshot};

/// Determines the simulated workload pattern.
///
/// # Abstract
/// This enum defines the behavior of the [`VmSimulator`] over time. By selecting
/// a profile, developers can reliably recreate specific stress conditions
/// (like resource exhaustion or CPU pinning) to ensure their telemetry
/// pipelines and alerting systems react appropriately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadProfile {
    /// Simulates a sudden, aggressive spike in CPU usage, eventually reaching
    /// near 100% saturation. Ideal for testing threshold alerts.
    CpuSpike,
    /// Simulates a steady, unbounded increase in memory usage over time.
    /// Useful for observing out-of-memory (OOM) prediction logic.
    MemoryLeak,
    /// Simulates normal, baseline idle operations with minimal variance.
    Idle,
}

/// A simulator that generates synthetic VM telemetry over time.
///
/// # Abstract
/// `VmSimulator` acts as an infinite stream of [`MetricsSnapshot`] objects,
/// tailored to the specific [`WorkloadProfile`] provided at creation. Because
/// it implements the standard library's `Iterator` trait, you can easily compose
/// it with other iterator adapters.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{VmSimulator, WorkloadProfile};
///
/// let mut sim = VmSimulator::new(WorkloadProfile::Idle);
/// let snap = sim.next().unwrap();
///
/// assert_eq!(snap.cpu.total_pct, 5.0);
/// ```
#[derive(Debug)]
pub struct VmSimulator {
    profile: WorkloadProfile,
    iteration: u64,
}

impl VmSimulator {
    /// Creates a new `VmSimulator` with the specified profile.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hitz_api::{VmSimulator, WorkloadProfile};
    ///
    /// let sim = VmSimulator::new(WorkloadProfile::MemoryLeak);
    /// ```
    #[must_use]
    pub const fn new(profile: WorkloadProfile) -> Self {
        Self {
            profile,
            iteration: 0,
        }
    }
}

impl Iterator for VmSimulator {
    type Item = MetricsSnapshot;

    fn next(&mut self) -> Option<Self::Item> {
        let (cpu_pct, mem_used) = match self.profile {
            WorkloadProfile::CpuSpike => {
                // Spike CPU aggressively, reaching ~95% eventually
                #[allow(clippy::cast_precision_loss)]
                let pct = (self.iteration as f32 * 15.0).min(95.0);
                (pct, 100 * 1024 * 1024)
            }
            WorkloadProfile::MemoryLeak => {
                // Steady leak, 50MB per iteration
                (10.0, 100 * 1024 * 1024 + self.iteration * 50 * 1024 * 1024)
            }
            WorkloadProfile::Idle => {
                // Idle baseline
                (5.0, 100 * 1024 * 1024)
            }
        };

        self.iteration += 1;

        Some(MetricsSnapshot {
            timestamp_ms: self.iteration * 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: mem_used,
                free_bytes: 1024 * 1024 * 1024 - mem_used,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_spike_profile() {
        let mut sim = VmSimulator::new(WorkloadProfile::CpuSpike);
        let mut max_cpu = 0.0;
        for _ in 0..10 {
            #[allow(clippy::collapsible_if)]
            if let Some(metrics) = sim.next() {
                if metrics.cpu.total_pct > max_cpu {
                    max_cpu = metrics.cpu.total_pct;
                }
            }
        }
        assert!(
            max_cpu > 90.0,
            "CPU did not spike above 90%, max was {max_cpu}"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_memory_leak_profile() {
        let mut sim = VmSimulator::new(WorkloadProfile::MemoryLeak);
        let metrics_0 = sim.next().expect("next should return Some");
        let metrics_1 = sim.next().expect("next should return Some");
        let metrics_2 = sim.next().expect("next should return Some");

        assert!(
            metrics_1.memory.used_bytes > metrics_0.memory.used_bytes,
            "Memory used should increase between iteration 0 and 1"
        );
        assert!(
            metrics_2.memory.used_bytes > metrics_1.memory.used_bytes,
            "Memory used should increase between iteration 1 and 2"
        );
    }

    #[test]
    #[allow(clippy::expect_used, clippy::float_cmp)]
    fn test_idle_profile() {
        let mut sim = VmSimulator::new(WorkloadProfile::Idle);
        let metrics_0 = sim.next().expect("next should return Some");
        let metrics_1 = sim.next().expect("next should return Some");

        assert_eq!(
            metrics_0.cpu.total_pct, metrics_1.cpu.total_pct,
            "CPU percentage should be stable for Idle profile"
        );
        assert_eq!(
            metrics_0.memory.used_bytes, metrics_1.memory.used_bytes,
            "Memory usage should be stable for Idle profile"
        );
    }

    #[cfg(feature = "health_check")]
    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_simulator_health_check_integration() {
        use crate::{HealthCheck, HealthStatus};

        let mut sim = VmSimulator::new(WorkloadProfile::CpuSpike);

        // Iteration 0: Idle baseline (cpu ~0)
        let metrics_0 = sim.next().unwrap();
        assert_eq!(metrics_0.assess_health().status, HealthStatus::Healthy);

        // Advance to a point where CPU spikes over 90%
        let mut critical_found = false;
        for _ in 0..10 {
            let metrics = sim.next().unwrap();
            if metrics.assess_health().status == HealthStatus::Critical {
                critical_found = true;
                break;
            }
        }

        assert!(
            critical_found,
            "The CPU spike should eventually trigger a Critical health status."
        );
    }
}
