//! Time-to-OOM Predictor for micro-VM telemetry.
//!
//! # Abstract
//! This module estimates when a VM will run out of memory based on historical
//! memory usage trends. By tracking recent metrics, we can provide an early warning
//! before an Out-Of-Memory (OOM) event occurs.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{OomPredictor, MetricsSnapshot, MemoryMetrics, CpuMetrics};
//!
//! let mut predictor = OomPredictor::new(3);
//!
//! // Snapshot 1: 100MB used
//! predictor.add_snapshot(&MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     memory: MemoryMetrics { used_bytes: 100_000_000, total_bytes: 500_000_000, free_bytes: 0, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     cpu: CpuMetrics { total_pct: 0.0, per_core: vec![], load_avg: [0.0; 3] },
//!     disks: vec![], networks: vec![], processes: vec![]
//! });
//!
//! // Snapshot 2: 200MB used
//! predictor.add_snapshot(&MetricsSnapshot {
//!     timestamp_ms: 2000,
//!     memory: MemoryMetrics { used_bytes: 200_000_000, total_bytes: 500_000_000, free_bytes: 0, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     cpu: CpuMetrics { total_pct: 0.0, per_core: vec![], load_avg: [0.0; 3] },
//!     disks: vec![], networks: vec![], processes: vec![]
//! });
//!
//! let prediction = predictor.predict();
//! assert!(prediction.time_to_oom_ms.is_some());
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The result of an OOM prediction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OomPrediction {
    /// Estimated milliseconds until the VM runs out of memory.
    /// Returns `None` if memory usage is stable or decreasing.
    pub time_to_oom_ms: Option<u64>,
    /// Estimated memory growth rate in bytes per millisecond.
    pub growth_rate_bytes_per_ms: Option<i64>,
}

/// Predicts when a VM will run out of memory by analyzing recent metrics.
#[derive(Debug, Clone)]
pub struct OomPredictor {
    history: Vec<(u64, u64)>,
    max_history_size: usize,
    total_memory: u64,
}

impl OomPredictor {
    /// Creates a new `OomPredictor` that keeps up to `max_history_size` historical data points.
    #[must_use]
    pub fn new(max_history_size: usize) -> Self {
        Self {
            history: Vec::with_capacity(max_history_size),
            max_history_size,
            total_memory: 0,
        }
    }

    /// Adds a new telemetry snapshot to the history.
    pub fn add_snapshot(&mut self, snapshot: &MetricsSnapshot) {
        if self.history.len() >= self.max_history_size {
            let _ = self.history.remove(0);
        }
        self.history.push((snapshot.timestamp_ms, snapshot.memory.used_bytes));
        self.total_memory = snapshot.memory.total_bytes;
    }

    /// Predicts the time until an OOM event occurs.
    #[must_use]
    #[allow(
        clippy::expect_used,
        clippy::missing_panics_doc,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn predict(&self) -> OomPrediction {
        if self.history.len() < 2 {
            return OomPrediction {
                time_to_oom_ms: None,
                growth_rate_bytes_per_ms: None,
            };
        }

        // safe to unwrap since we checked len > 1
        let first = self.history.first().expect("should have at least 2 elements");
        let last = self.history.last().expect("should have at least 2 elements");

        if last.0 <= first.0 {
            return OomPrediction {
                time_to_oom_ms: None,
                growth_rate_bytes_per_ms: None,
            };
        }

        let time_delta_ms = last.0 - first.0;
        let mem_delta_bytes = last.1 as i64 - first.1 as i64;

        let rate = mem_delta_bytes / (time_delta_ms as i64);

        if rate <= 0 {
            return OomPrediction {
                time_to_oom_ms: None,
                growth_rate_bytes_per_ms: Some(rate),
            };
        }

        let remaining_bytes = self.total_memory.saturating_sub(last.1);
        let time_to_oom_ms = remaining_bytes / (rate as u64);

        OomPrediction {
            time_to_oom_ms: Some(time_to_oom_ms),
            growth_rate_bytes_per_ms: Some(rate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn make_snapshot(timestamp_ms: u64, used_bytes: u64, total_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms,
            memory: MemoryMetrics {
                total_bytes,
                used_bytes,
                free_bytes: total_bytes.saturating_sub(used_bytes),
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            cpu: CpuMetrics {
                total_pct: 0.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        }
    }

    #[test]
    fn test_predict_stable_memory() {
        let mut predictor = OomPredictor::new(5);
        predictor.add_snapshot(&make_snapshot(1000, 100, 1000));
        predictor.add_snapshot(&make_snapshot(2000, 100, 1000));

        let prediction = predictor.predict();
        assert_eq!(prediction.time_to_oom_ms, None);
        assert_eq!(prediction.growth_rate_bytes_per_ms, Some(0));
    }

    #[test]
    fn test_predict_leaking_memory() {
        let mut predictor = OomPredictor::new(5);
        predictor.add_snapshot(&make_snapshot(1000, 100, 1000));
        predictor.add_snapshot(&make_snapshot(2000, 10100, 20100)); // Leak 10 bytes per ms

        let prediction = predictor.predict();
        assert_eq!(prediction.growth_rate_bytes_per_ms, Some(10));
        assert_eq!(prediction.time_to_oom_ms, Some(1000)); // (20100 - 10100) / 10 = 1000
    }
}
