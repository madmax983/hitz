//! Scoring module for calculating a VM's overall fitness score.
//!
//! # Abstract
//! This module provides the tools necessary to evaluate the overall fitness
//! or efficiency of a micro-VM based on its telemetry metrics. Unlike the
//! `health` module which checks for critical failures, the `scoring` module
//! returns a normalized score (0-100) indicating how efficiently the VM is
//! utilizing its resources.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::scoring::FitnessScore;
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // We have some metrics from our micro-VM
//! let metrics = MetricsSnapshot {
//!     timestamp_ms: 1_700_000_000_000,
//!     cpu: CpuMetrics { total_pct: 45.0, per_core: vec![45.0], load_avg: [0.5, 0.5, 0.5] },
//!     memory: MemoryMetrics { total_bytes: 100, used_bytes: 40, free_bytes: 60, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // We calculate the fitness score
//! let score = metrics.calculate_fitness();
//!
//! // A perfectly balanced system gets a high score!
//! assert!(score > 80.0);
//! ```

use crate::MetricsSnapshot;

/// Trait for types that can calculate their own fitness score.
pub trait FitnessScore {
    /// Calculates a normalized fitness score from 0.0 to 100.0.
    /// Higher is better. A score near 100 indicates perfect resource utilization
    /// (neither idle nor overloaded).
    fn calculate_fitness(&self) -> f64;
}

impl FitnessScore for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_fitness(&self) -> f64 {
        // CPU Scoring
        // We define "ideal" CPU usage around 50-70%.
        // Too low means under-utilized. Too high means overloaded.
        let cpu_score = if self.cpu.total_pct < 10.0 {
            40.0 // Idle
        } else if self.cpu.total_pct < 40.0 {
            70.0 // Underutilized
        } else if self.cpu.total_pct < 80.0 {
            100.0 // Ideal
        } else if self.cpu.total_pct < 95.0 {
            60.0 // Stressed
        } else {
            10.0 // Overloaded
        };

        // Memory Scoring
        let mem_score = if self.memory.total_bytes > 0 {
            let used_pct = (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            if used_pct < 20.0 {
                50.0 // Overprovisioned
            } else if used_pct < 75.0 {
                100.0 // Ideal
            } else if used_pct < 90.0 {
                60.0 // Getting tight
            } else {
                20.0 // OOM Risk
            }
        } else {
            100.0 // No memory data, don't penalize
        };

        // Network Error Penalty
        let total_net_errors: u64 = self
            .networks
            .iter()
            .map(|n| n.rx_errors + n.tx_errors)
            .sum();
        let net_penalty = if total_net_errors > 100 {
            30.0
        } else if total_net_errors > 10 {
            10.0
        } else {
            0.0
        };

        // Weighted Average
        let base_score: f64 = (cpu_score * 0.5) + (mem_score * 0.5);
        (base_score - net_penalty).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, NetMetrics};

    fn base_metrics() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 60.0, // Ideal
                per_core: vec![60.0],
                load_avg: [0.5, 0.5, 0.5],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 500, // Ideal (50%)
                free_bytes: 500,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
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
    fn test_ideal_fitness() {
        let metrics = base_metrics();
        assert_eq!(metrics.calculate_fitness(), 100.0);
    }

    #[test]
    fn test_idle_fitness() {
        let mut metrics = base_metrics();
        metrics.cpu.total_pct = 5.0; // Idle
        metrics.memory.used_bytes = 100; // Overprovisioned (10%)

        let score = metrics.calculate_fitness();
        // CPU 40 * 0.5 = 20. Mem 50 * 0.5 = 25. Total = 45.
        assert_eq!(score, 45.0);
    }

    #[test]
    fn test_overloaded_fitness() {
        let mut metrics = base_metrics();
        metrics.cpu.total_pct = 99.0; // Overloaded
        metrics.memory.used_bytes = 950; // OOM Risk (95%)

        let score = metrics.calculate_fitness();
        // CPU 10 * 0.5 = 5. Mem 20 * 0.5 = 10. Total = 15.
        assert_eq!(score, 15.0);
    }

    #[test]
    fn test_network_penalty() {
        let mut metrics = base_metrics();
        metrics.networks[0].rx_errors = 50; // Triggers 10.0 penalty

        let score = metrics.calculate_fitness();
        assert_eq!(score, 90.0);
    }
}
