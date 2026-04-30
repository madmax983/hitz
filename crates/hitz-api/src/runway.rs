//! Runway estimator module for predicting resource exhaustion.
//!
//! # Abstract
//! This module calculates the "runway" (time remaining) before a VM exhausts
//! critical resources, such as memory or CPU capacity, based on the current
//! rate of consumption between two [`MetricsSnapshot`]s.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{RunwayEstimator, RunwayResult, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // Snapshot 1: 100MB used
//! let snap1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1000, used_bytes: 100, free_bytes: 900, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // Snapshot 2: 200MB used (1 second later -> growth is 100MB/sec)
//! let mut snap2 = snap1.clone();
//! snap2.timestamp_ms = 2000;
//! snap2.memory.used_bytes = 200;
//!
//! let runway = snap2.estimate_runway(&snap1).unwrap();
//! // Remaining memory is 800MB. At 100MB/s, runway is 8.0 seconds!
//! assert_eq!(runway.memory_secs, Some(8.0));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The estimated runway (time remaining) before resource exhaustion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunwayResult {
    /// Estimated seconds until memory is completely exhausted.
    /// `None` if memory usage is stable or decreasing.
    pub memory_secs: Option<f64>,
    /// Estimated seconds until CPU utilization hits 100%.
    /// `None` if CPU utilization is stable or decreasing.
    pub cpu_secs: Option<f64>,
}

/// Trait to estimate resource runway based on historical consumption.
pub trait RunwayEstimator {
    /// Estimates the remaining time before resources are exhausted.
    /// Returns `None` if `previous` is newer than or equal to `self`.
    fn estimate_runway(&self, previous: &Self) -> Option<RunwayResult>;
}

impl RunwayEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_runway(&self, previous: &Self) -> Option<RunwayResult> {
        if self.timestamp_ms <= previous.timestamp_ms {
            return None;
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;

        // Memory Runway
        let memory_secs = if self.memory.used_bytes > previous.memory.used_bytes {
            let growth_bytes = (self.memory.used_bytes - previous.memory.used_bytes) as f64;
            let growth_rate_per_sec = growth_bytes / elapsed_secs;

            if growth_rate_per_sec > 0.0 {
                let remaining_bytes =
                    self.memory
                        .total_bytes
                        .saturating_sub(self.memory.used_bytes) as f64;
                Some(remaining_bytes / growth_rate_per_sec)
            } else {
                None
            }
        } else {
            None
        };

        // CPU Runway
        let cpu_secs = if self.cpu.total_pct > previous.cpu.total_pct {
            let growth_pct = f64::from(self.cpu.total_pct - previous.cpu.total_pct);
            let growth_rate_per_sec = growth_pct / elapsed_secs;

            if growth_rate_per_sec > 0.0 {
                let remaining_pct = (100.0_f64 - f64::from(self.cpu.total_pct)).max(0.0);
                Some(remaining_pct / growth_rate_per_sec)
            } else {
                None
            }
        } else {
            None
        };

        Some(RunwayResult {
            memory_secs,
            cpu_secs,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(time_ms: u64, cpu: f32, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: time_ms,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: mem_used,
                free_bytes: 1000u64.saturating_sub(mem_used),
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
    fn test_memory_runway_calculation() {
        let snap1 = dummy_snapshot(1000, 10.0, 100);
        let snap2 = dummy_snapshot(2000, 10.0, 200);

        let runway = snap2
            .estimate_runway(&snap1)
            .expect("Runway should be Some");

        // 100 bytes / sec. 800 bytes remaining.
        assert_eq!(runway.memory_secs, Some(8.0));
        assert_eq!(runway.cpu_secs, None); // CPU didn't grow
    }

    #[test]
    fn test_cpu_runway_calculation() {
        let snap1 = dummy_snapshot(1000, 10.0, 100);
        let snap2 = dummy_snapshot(2000, 20.0, 100);

        let runway = snap2
            .estimate_runway(&snap1)
            .expect("Runway should be Some");

        // 10% / sec. 80% remaining.
        assert_eq!(runway.cpu_secs, Some(8.0));
        assert_eq!(runway.memory_secs, None); // Memory didn't grow
    }

    #[test]
    fn test_no_runway_if_stable_or_decreasing() {
        let snap1 = dummy_snapshot(1000, 50.0, 500);
        let snap2 = dummy_snapshot(2000, 40.0, 400);

        let runway = snap2
            .estimate_runway(&snap1)
            .expect("Runway should be Some");

        assert_eq!(runway.memory_secs, None);
        assert_eq!(runway.cpu_secs, None);
    }

    #[test]
    fn test_invalid_timestamps() {
        let snap1 = dummy_snapshot(2000, 10.0, 100);
        let snap2 = dummy_snapshot(1000, 20.0, 200);

        assert!(snap2.estimate_runway(&snap1).is_none());
    }

    #[test]
    fn test_zero_elapsed_time() {
        let snap1 = dummy_snapshot(1000, 10.0, 100);
        let snap2 = dummy_snapshot(1000, 20.0, 200);

        assert!(snap2.estimate_runway(&snap1).is_none());
    }
}
