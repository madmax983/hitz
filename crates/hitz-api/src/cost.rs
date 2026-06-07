//! Cost estimation module.
//!
//! # Abstract
//! This module estimates the operational cost of a VM based on its resource usage
//! (`MetricsSnapshot`) and a configured `CostProfile`.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{CostEstimator, CostProfile, MetricsSnapshot};
//!
//! // let snap: MetricsSnapshot = ...;
//! // let profile = CostProfile::new(0.05, 0.01);
//! // let cost = snap.estimate_cost(&profile, 4);
//! // println!("Estimated cost: ${}/hr", cost);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable pricing parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostProfile {
    /// Cost per vCPU per hour.
    pub cpu_hourly_rate: f64,
    /// Cost per GB RAM per hour.
    pub ram_hourly_rate: f64,
}

impl CostProfile {
    /// Creates a new `CostProfile`.
    #[must_use]
    pub const fn new(cpu_hourly_rate: f64, ram_hourly_rate: f64) -> Self {
        Self {
            cpu_hourly_rate,
            ram_hourly_rate,
        }
    }
}

/// Trait to estimate operational cost.
pub trait CostEstimator {
    /// Estimates cost per hour based on current usage.
    fn estimate_cost(&self, profile: &CostProfile, vcpus: u32) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_cost(&self, profile: &CostProfile, vcpus: u32) -> f64 {
        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let vcpus_f64 = f64::from(vcpus);
        let active_cpus = vcpus_f64 * cpu_utilization;
        let cpu_cost = active_cpus * profile.cpu_hourly_rate;

        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = ram_gb * profile.ram_hourly_rate;

        cpu_cost + ram_cost
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(cpu_pct: f32, ram_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_bytes,
                used_bytes: ram_bytes / 2,
                free_bytes: ram_bytes / 2,
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
    fn test_cost_estimator() {
        let profile = CostProfile::new(0.05, 0.01);
        let snap = base_snapshot(100.0, 2 * 1024 * 1024 * 1024);
        let cost = snap.estimate_cost(&profile, 4);
        assert!((cost - 0.22).abs() < f64::EPSILON);
    }
}
