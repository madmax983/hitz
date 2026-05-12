//! CPU Prediction Module.
//!
//! # Abstract
//! This module provides a `VmHistory` struct that tracks recent `MetricsSnapshot`
//! instances and performs simple linear regression to predict future CPU utilization.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics, prediction::VmHistory};
//!
//! let mut history = VmHistory::new(10); // keep last 10 snapshots
//!
//! let mut snap1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let mut snap2 = snap1.clone();
//! snap2.timestamp_ms = 2000;
//! snap2.cpu.total_pct = 20.0;
//!
//! history.push(snap1);
//! history.push(snap2);
//!
//! // Predict CPU usage at timestamp 3000
//! let predicted_cpu = history.predict_cpu_pct(3000);
//! assert!(predicted_cpu > 25.0 && predicted_cpu < 35.0);
//! ```

use crate::MetricsSnapshot;
use std::collections::VecDeque;

/// Tracks a time-series of metrics snapshots to predict future utilization.
#[derive(Debug, Clone)]
pub struct VmHistory {
    snapshots: VecDeque<MetricsSnapshot>,
    capacity: usize,
}

impl VmHistory {
    /// Creates a new `VmHistory` bounded to keep only the `capacity` most recent snapshots.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            snapshots: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Adds a new snapshot, dropping the oldest if at capacity.
    pub fn push(&mut self, snapshot: MetricsSnapshot) {
        if self.snapshots.len() == self.capacity {
            let _ = self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);
    }

    /// Predicts the CPU `total_pct` at a future `target_timestamp_ms` using linear regression.
    ///
    /// If there are fewer than 2 snapshots, it returns the most recent CPU usage (or 0.0).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn predict_cpu_pct(&self, target_timestamp_ms: u64) -> f64 {
        let n = self.snapshots.len() as f64;
        if n < 2.0 {
            return self
                .snapshots
                .back()
                .map_or(0.0, |s| f64::from(s.cpu.total_pct));
        }

        // Normalize time to avoid huge floating point values for X.
        // We set the first snapshot's timestamp as t=0.
        let base_time = self.snapshots.front().map_or(0, |s| s.timestamp_ms);

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_x_y = 0.0;
        let mut sum_x2 = 0.0;

        for s in &self.snapshots {
            let x = (s.timestamp_ms - base_time) as f64;
            let y = f64::from(s.cpu.total_pct);

            sum_x += x;
            sum_y += y;
            sum_x_y += x * y;
            sum_x2 += x * x;
        }

        let denominator = n.mul_add(sum_x2, -(sum_x * sum_x));
        if denominator == 0.0 {
            // All timestamps are the same (shouldn't happen in practice, but prevent div by 0).
            return self
                .snapshots
                .back()
                .map_or(0.0, |s| f64::from(s.cpu.total_pct));
        }

        let slope = (n.mul_add(sum_x_y, -(sum_x * sum_y))) / denominator;
        let intercept = (sum_y - slope * sum_x) / n;

        let target_x = (target_timestamp_ms.saturating_sub(base_time)) as f64;
        let predicted_y = slope * target_x + intercept;

        // Clamp to valid CPU range
        predicted_y.clamp(0.0, 100.0)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(timestamp: u64, cpu: f32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: timestamp,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
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
    fn test_push_capacity() {
        let mut history = VmHistory::new(2);
        history.push(dummy_snapshot(1000, 10.0));
        history.push(dummy_snapshot(2000, 20.0));
        assert_eq!(history.snapshots.len(), 2);

        // Pushing a 3rd should evict the 1st
        history.push(dummy_snapshot(3000, 30.0));
        assert_eq!(history.snapshots.len(), 2);
        assert_eq!(history.snapshots.front().unwrap().timestamp_ms, 2000);
    }

    #[test]
    fn test_predict_insufficient_data() {
        let mut history = VmHistory::new(5);
        // 0 snapshots -> returns 0.0
        assert_eq!(history.predict_cpu_pct(1000), 0.0);

        // 1 snapshot -> returns the only value
        history.push(dummy_snapshot(1000, 42.0));
        assert_eq!(history.predict_cpu_pct(2000), 42.0);
    }

    #[test]
    fn test_predict_linear_increase() {
        let mut history = VmHistory::new(5);
        history.push(dummy_snapshot(1000, 10.0));
        history.push(dummy_snapshot(2000, 20.0));
        history.push(dummy_snapshot(3000, 30.0));

        // Predicting for 4000 should give 40.0
        let pred = history.predict_cpu_pct(4000);
        assert!((pred - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_predict_clamped_output() {
        let mut history = VmHistory::new(5);
        history.push(dummy_snapshot(1000, 80.0));
        history.push(dummy_snapshot(2000, 90.0));
        history.push(dummy_snapshot(3000, 100.0));

        // Predict for 4000. Trend says 110.0, but it should clamp to 100.0.
        let pred = history.predict_cpu_pct(4000);
        assert_eq!(pred, 100.0);

        // Downward trend testing lower clamp
        let mut history_down = VmHistory::new(5);
        history_down.push(dummy_snapshot(1000, 20.0));
        history_down.push(dummy_snapshot(2000, 10.0));
        history_down.push(dummy_snapshot(3000, 0.0));

        let pred_down = history_down.predict_cpu_pct(4000);
        assert_eq!(pred_down, 0.0);
    }
}
