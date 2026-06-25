//! Predict time-to-exhaustion (TTE) for VM resources.
//!
//! # Abstract
//! This module calculates the velocity of resource consumption (CPU and Memory)
//! between two telemetry snapshots. It extrapolates this velocity to estimate
//! how many seconds remain before the VM runs out of resources.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics, PredictExhaustion};
//!
//! let snap1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 50.0, per_core: vec![], load_avg: [0.0, 0.0, 0.0] },
//!     memory: MemoryMetrics { total_bytes: 100, used_bytes: 50, free_bytes: 50, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let snap2 = MetricsSnapshot {
//!     timestamp_ms: 2000,
//!     cpu: CpuMetrics { total_pct: 60.0, per_core: vec![], load_avg: [0.0, 0.0, 0.0] },
//!     memory: MemoryMetrics { total_bytes: 100, used_bytes: 60, free_bytes: 40, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let prediction = snap2.predict_exhaustion(&snap1).unwrap();
//!
//! // Memory usage increased by 10 bytes in 1 second.
//! // 40 bytes remain, so it will exhaust in 4 seconds.
//! assert_eq!(prediction.memory_seconds, Some(4.0));
//!
//! // CPU usage increased by 10% in 1 second.
//! // 40% remain until 100%, so it will exhaust in 4 seconds.
//! assert_eq!(prediction.cpu_seconds, Some(4.0));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The predicted time until resource exhaustion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExhaustionPrediction {
    /// Seconds until CPU reaches 100%. `None` if usage is decreasing or stable.
    pub cpu_seconds: Option<f64>,
    /// Seconds until Memory usage equals Total Memory. `None` if usage is decreasing or stable.
    pub memory_seconds: Option<f64>,
}

/// Trait to predict when resources will be exhausted based on historical usage.
pub trait PredictExhaustion {
    /// Predicts time-to-exhaustion by comparing current state against a previous state.
    /// Returns `None` if `previous` is not older than `self`.
    fn predict_exhaustion(&self, previous: &MetricsSnapshot) -> Option<ExhaustionPrediction>;
}

impl PredictExhaustion for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn predict_exhaustion(&self, previous: &MetricsSnapshot) -> Option<ExhaustionPrediction> {
        if self.timestamp_ms <= previous.timestamp_ms {
            return None;
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;

        // CPU Extrapolation
        let cpu_diff = self.cpu.total_pct - previous.cpu.total_pct;
        let cpu_seconds = if cpu_diff > 0.0 {
            let cpu_rate = f64::from(cpu_diff) / elapsed_secs;
            let cpu_remaining = f64::from(100.0 - self.cpu.total_pct);
            Some(cpu_remaining / cpu_rate)
        } else {
            None
        };

        // Memory Extrapolation
        let memory_diff = self.memory.used_bytes as f64 - previous.memory.used_bytes as f64;
        let memory_seconds = if memory_diff > 0.0 {
            let mem_rate = memory_diff / elapsed_secs;
            let mem_remaining = (self.memory.total_bytes - self.memory.used_bytes) as f64;
            Some(mem_remaining / mem_rate)
        } else {
            None
        };

        Some(ExhaustionPrediction {
            cpu_seconds,
            memory_seconds,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn make_snap(ts: u64, cpu: f32, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: ts,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total,
                used_bytes: mem_used,
                free_bytes: mem_total - mem_used,
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
    fn test_predict_exhaustion() {
        let snap1 = make_snap(1000, 50.0, 100, 50);
        let snap2 = make_snap(2000, 60.0, 100, 60);

        let pred = snap2.predict_exhaustion(&snap1).expect("should predict");

        // CPU: 10% per second. 40% remaining -> 4.0 seconds.
        assert_eq!(pred.cpu_seconds, Some(4.0));
        // Memory: 10 bytes per second. 40 bytes remaining -> 4.0 seconds.
        assert_eq!(pred.memory_seconds, Some(4.0));
    }

    #[test]
    fn test_predict_exhaustion_decreasing() {
        let snap1 = make_snap(1000, 50.0, 100, 50);
        let snap2 = make_snap(2000, 40.0, 100, 40);

        let pred = snap2.predict_exhaustion(&snap1).expect("should predict");

        // CPU is decreasing
        assert_eq!(pred.cpu_seconds, None);
        // Memory is decreasing
        assert_eq!(pred.memory_seconds, None);
    }

    #[test]
    fn test_predict_exhaustion_stable() {
        let snap1 = make_snap(1000, 50.0, 100, 50);
        let snap2 = make_snap(2000, 50.0, 100, 50);

        let pred = snap2.predict_exhaustion(&snap1).expect("should predict");

        // CPU is stable
        assert_eq!(pred.cpu_seconds, None);
        // Memory is stable
        assert_eq!(pred.memory_seconds, None);
    }

    #[test]
    fn test_predict_exhaustion_invalid_time() {
        let snap1 = make_snap(2000, 50.0, 100, 50);
        let snap2 = make_snap(1000, 60.0, 100, 60);

        assert_eq!(snap2.predict_exhaustion(&snap1), None);
    }

    #[test]
    fn test_predict_exhaustion_same_time() {
        let snap1 = make_snap(1000, 50.0, 100, 50);
        let snap2 = make_snap(1000, 60.0, 100, 60);

        assert_eq!(snap2.predict_exhaustion(&snap1), None);
    }
}
