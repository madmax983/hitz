//! Out of Memory (OOM) prediction module.
//!
//! # Abstract
//! This module estimates the Time-To-OOM (TTO) based on the current memory usage
//! and the historical rate of memory growth.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{OomPredictor, OomPrediction, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let mut previous = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1000, used_bytes: 500, free_bytes: 500, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let mut current = previous.clone();
//! current.timestamp_ms = 2000; // 1 second later
//! current.memory.used_bytes = 600; // leaked 100 bytes
//!
//! let prediction = current.predict_oom(&previous);
//! // 400 bytes remaining / 100 bytes per second = 4.0 seconds until OOM
//! assert_eq!(prediction.time_to_oom_secs, Some(4.0));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Prediction result containing the estimated time until memory is exhausted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OomPrediction {
    /// Estimated time to OOM in seconds. `None` if memory is stable or decreasing.
    pub time_to_oom_secs: Option<f64>,
    /// The calculated leak rate in bytes per second.
    pub leak_rate_bytes_per_sec: f64,
}

/// Trait for objects that can predict an Out of Memory event.
pub trait OomPredictor {
    /// Predicts the time until OOM given a previous snapshot.
    fn predict_oom(&self, previous: &Self) -> OomPrediction;
}

impl OomPredictor for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn predict_oom(&self, previous: &Self) -> OomPrediction {
        if self.timestamp_ms <= previous.timestamp_ms {
            return OomPrediction {
                time_to_oom_secs: None,
                leak_rate_bytes_per_sec: 0.0,
            };
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;
        let current_used = self.memory.used_bytes as f64;
        let previous_used = previous.memory.used_bytes as f64;

        let leak_rate = (current_used - previous_used) / elapsed_secs;

        if leak_rate <= 0.0 || self.memory.total_bytes == 0 {
            return OomPrediction {
                time_to_oom_secs: None,
                leak_rate_bytes_per_sec: leak_rate.max(0.0),
            };
        }

        let remaining_bytes = self.memory.total_bytes.saturating_sub(self.memory.used_bytes) as f64;
        let time_to_oom = remaining_bytes / leak_rate;

        OomPrediction {
            time_to_oom_secs: Some(time_to_oom),
            leak_rate_bytes_per_sec: leak_rate,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(time_ms: u64, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: time_ms,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total,
                used_bytes: mem_used,
                free_bytes: mem_total.saturating_sub(mem_used),
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
    fn test_memory_leak() {
        let prev = dummy_snapshot(1000, 1000, 500);
        let curr = dummy_snapshot(2000, 1000, 600); // +100 bytes in 1s

        let prediction = curr.predict_oom(&prev);
        assert_eq!(prediction.leak_rate_bytes_per_sec, 100.0);
        assert_eq!(prediction.time_to_oom_secs, Some(4.0)); // 400 remaining / 100 per sec
    }

    #[test]
    fn test_stable_memory() {
        let prev = dummy_snapshot(1000, 1000, 500);
        let curr = dummy_snapshot(2000, 1000, 500); // 0 bytes leaked

        let prediction = curr.predict_oom(&prev);
        assert_eq!(prediction.leak_rate_bytes_per_sec, 0.0);
        assert_eq!(prediction.time_to_oom_secs, None);
    }

    #[test]
    fn test_decreasing_memory() {
        let prev = dummy_snapshot(1000, 1000, 500);
        let curr = dummy_snapshot(2000, 1000, 400); // -100 bytes in 1s

        let prediction = curr.predict_oom(&prev);
        assert_eq!(prediction.leak_rate_bytes_per_sec, 0.0); // Clamped to 0
        assert_eq!(prediction.time_to_oom_secs, None);
    }

    #[test]
    fn test_zero_elapsed_time() {
        let prev = dummy_snapshot(1000, 1000, 500);
        let curr = dummy_snapshot(1000, 1000, 600);

        let prediction = curr.predict_oom(&prev);
        assert_eq!(prediction.leak_rate_bytes_per_sec, 0.0);
        assert_eq!(prediction.time_to_oom_secs, None);
    }

    #[test]
    fn test_zero_total_memory() {
        let prev = dummy_snapshot(1000, 0, 0);
        let curr = dummy_snapshot(2000, 0, 100);

        let prediction = curr.predict_oom(&prev);
        assert_eq!(prediction.time_to_oom_secs, None);
    }
}
