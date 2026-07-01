//! Workload trend forecasting module.
//!
//! # Abstract
//! This module attempts to forecast future resource utilization based on a
//! historical window of `MetricsSnapshot`s using simple linear regression.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{TrendForecaster, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let mut forecaster = TrendForecaster::new(3);
//!
//! // Add some synthetic snapshots showing increasing CPU usage
//! for i in 1..=3 {
//!     let snap = MetricsSnapshot {
//!         timestamp_ms: i * 1000,
//!         cpu: CpuMetrics {
//!             total_pct: i as f32 * 10.0,
//!             per_core: vec![i as f32 * 10.0],
//!             load_avg: [0.1, 0.1, 0.1],
//!         },
//!         memory: MemoryMetrics {
//!             total_bytes: 1024, used_bytes: 512, free_bytes: 512,
//!             buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
//!         },
//!         disks: vec![],
//!         networks: vec![],
//!         processes: vec![],
//!     };
//!     forecaster.add_snapshot(snap);
//! }
//!
//! // Predict the CPU usage for the next step (timestamp 4000)
//! let predicted_cpu = forecaster.forecast_cpu(4000).unwrap();
//! assert!((predicted_cpu - 40.0).abs() < f32::EPSILON);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// A forecaster that predicts future resource utilization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendForecaster {
    /// The maximum number of historical snapshots to retain.
    pub window_size: usize,
    /// The retained history of metrics.
    pub history: Vec<MetricsSnapshot>,
}

impl TrendForecaster {
    /// Creates a new `TrendForecaster` with the given window size.
    #[must_use]
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            history: Vec::with_capacity(window_size),
        }
    }

    /// Adds a snapshot to the historical window.
    pub fn add_snapshot(&mut self, snap: MetricsSnapshot) {
        if self.history.len() >= self.window_size {
            let _ = self.history.remove(0);
        }
        self.history.push(snap);
    }

    /// Forecasts the CPU utilization percentage at the given future timestamp using linear regression.
    /// Returns `None` if there is insufficient history (less than 2 data points) or if all timestamps are identical.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::similar_names,
        clippy::suspicious_operation_groupings,
        clippy::suboptimal_flops,
        clippy::cast_possible_truncation
    )]
    pub fn forecast_cpu(&self, target_timestamp_ms: u64) -> Option<f32> {
        let n = self.history.len();
        if n < 2 {
            return None;
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for snap in &self.history {
            let x = snap.timestamp_ms as f64;
            let y = f64::from(snap.cpu.total_pct);
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let n_f64 = n as f64;
        let denominator = n_f64 * sum_xx - sum_x * sum_x;

        // Handle vertical line (all timestamps identical)
        if denominator.abs() < f64::EPSILON {
            return None;
        }

        let slope = (n_f64 * sum_xy - sum_x * sum_y) / denominator;
        let intercept = (sum_y - slope * sum_x) / n_f64;

        let target_x = target_timestamp_ms as f64;
        let predicted_y = slope * target_x + intercept;

        Some(predicted_y.clamp(0.0, 100.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snap(ts: u64, cpu: f32) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: ts,
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
    fn test_forecast_insufficient_data() {
        let mut f = TrendForecaster::new(5);
        assert!(f.forecast_cpu(1000).is_none());
        f.add_snapshot(dummy_snap(1000, 10.0));
        assert!(f.forecast_cpu(2000).is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_forecast_linear_trend() {
        let mut f = TrendForecaster::new(3);
        f.add_snapshot(dummy_snap(1000, 10.0));
        f.add_snapshot(dummy_snap(2000, 20.0));
        f.add_snapshot(dummy_snap(3000, 30.0));

        let p = f.forecast_cpu(4000).unwrap();
        assert!((p - 40.0).abs() < f32::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_forecast_clamped() {
        let mut f = TrendForecaster::new(3);
        f.add_snapshot(dummy_snap(1000, 50.0));
        f.add_snapshot(dummy_snap(2000, 100.0)); // Extremely fast growth

        // Would naturally exceed 100, but clamped to 100
        let p = f.forecast_cpu(3000).unwrap();
        assert!((p - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_forecast_denominator_zero() {
        let mut f = TrendForecaster::new(3);
        f.add_snapshot(dummy_snap(1000, 10.0));
        f.add_snapshot(dummy_snap(1000, 20.0));
        assert!(f.forecast_cpu(2000).is_none());
    }
}
