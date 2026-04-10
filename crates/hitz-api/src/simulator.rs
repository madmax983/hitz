use crate::{CpuMetrics, MemoryMetrics, MetricsSnapshot};

/// Determines the simulated workload pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadProfile {
    /// Simulates a sudden spike in CPU usage.
    CpuSpike,
    /// Simulates a steady increase in memory usage.
    MemoryLeak,
    /// Simulates normal idle operations.
    Idle,
}

/// A simulator that generates synthetic VM telemetry over time.
#[derive(Debug)]
pub struct VmSimulator {
    profile: WorkloadProfile,
    iteration: u64,
}

impl VmSimulator {
    /// Creates a new `VmSimulator` with the specified profile.
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
        #[allow(clippy::collapsible_if)]
        for _ in 0..10 {
            if let Some(metrics) = sim.next() {
                if metrics.cpu.total_pct > max_cpu {
                    max_cpu = metrics.cpu.total_pct;
                }
            }
        }
        assert!(max_cpu > 90.0, "CPU did not spike above 90%, max was {max_cpu}");
    }

    #[cfg(feature = "health_check")]
    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_simulator_health_check_integration() {
        use crate::health::{HealthCheck, HealthStatus};

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
