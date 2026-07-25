//! Trend prediction module for forecasting resource exhaustion.
//!
//! # Abstract
//! Predicts CPU and memory exhaustion in the near future based on history.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{TrendPredictor, TrendPrediction, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let history = vec![
//!     MetricsSnapshot {
//!         timestamp_ms: 1000,
//!         cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!         memory: MemoryMetrics {
//!             total_bytes: 1000,
//!             used_bytes: 500,
//!             free_bytes: 500,
//!             buffers_bytes: 0,
//!             cached_bytes: 0,
//!             swap_total: 0,
//!             swap_used: 0,
//!         },
//!         disks: vec![],
//!         networks: vec![],
//!         processes: vec![],
//!     },
//!     MetricsSnapshot {
//!         timestamp_ms: 2000,
//!         cpu: CpuMetrics { total_pct: 20.0, per_core: vec![20.0], load_avg: [0.2, 0.2, 0.2] },
//!         memory: MemoryMetrics {
//!             total_bytes: 1000,
//!             used_bytes: 750,
//!             free_bytes: 250,
//!             buffers_bytes: 0,
//!             cached_bytes: 0,
//!             swap_total: 0,
//!             swap_used: 0,
//!         },
//!         disks: vec![],
//!         networks: vec![],
//!         processes: vec![],
//!     }
//! ];
//!
//! let pred = history.predict_trends(1000).expect("prediction failed");
//! assert!(pred.is_oom_imminent);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// A trend prediction for future resource usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendPrediction {
    /// Predicted memory percentage usage at the given horizon.
    pub predicted_mem_pct: f64,
    /// Predicted CPU percentage at the given horizon.
    pub predicted_cpu_pct: f64,
    /// Whether an out-of-memory condition is predicted.
    pub is_oom_imminent: bool,
    /// Whether a CPU spike is predicted.
    pub is_cpu_spike: bool,
}

/// Trait to predict trends from a history of metrics snapshots.
pub trait TrendPredictor {
    /// Predicts future resource usage based on historical data. `horizon_ms` is the number of milliseconds into the future to predict.
    fn predict_trends(&self, horizon_ms: u64) -> Option<TrendPrediction>;
}

impl TrendPredictor for [MetricsSnapshot] {
    #[allow(clippy::cast_precision_loss)]
    fn predict_trends(&self, horizon_ms: u64) -> Option<TrendPrediction> {
        if self.len() < 2 {
            return None;
        }

        let last = self.last()?;
        let first = self.first()?;

        let time_diff = last.timestamp_ms.saturating_sub(first.timestamp_ms);
        if time_diff == 0 {
            return None;
        }

        let time_diff_f64 = time_diff as f64;
        let horizon_f64 = horizon_ms as f64;

        // CPU Prediction
        let cpu_diff = last.cpu.total_pct - first.cpu.total_pct;
        let cpu_rate = f64::from(cpu_diff) / time_diff_f64;
        let predicted_cpu_pct = cpu_rate.mul_add(horizon_f64, f64::from(last.cpu.total_pct)).clamp(0.0, 100.0);

        // Memory Prediction
        let mem_pct_last = if last.memory.total_bytes > 0 {
            (last.memory.used_bytes as f64 / last.memory.total_bytes as f64) * 100.0
        } else {
            0.0
        };
        let mem_pct_first = if first.memory.total_bytes > 0 {
            (first.memory.used_bytes as f64 / first.memory.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let mem_diff = mem_pct_last - mem_pct_first;
        let mem_rate = mem_diff / time_diff_f64;
        let predicted_mem_pct = mem_rate.mul_add(horizon_f64, mem_pct_last).clamp(0.0, 100.0);

        Some(TrendPrediction {
            predicted_mem_pct,
            predicted_cpu_pct,
            is_oom_imminent: predicted_mem_pct >= 95.0,
            is_cpu_spike: predicted_cpu_pct >= 95.0,
        })
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn snap(time: u64, cpu: f32, mem_pct: f64) -> MetricsSnapshot {
        let total_mem = 1_000_000;
        let used_mem = (total_mem as f64 * (mem_pct / 100.0)) as u64;

        MetricsSnapshot {
            timestamp_ms: time,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: total_mem,
                used_bytes: used_mem,
                free_bytes: total_mem - used_mem,
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
    fn test_predict_trends_insufficient_data() {
        let history = [snap(1000, 10.0, 10.0)];
        assert!(history.predict_trends(1000).is_none());
    }

    #[test]
    #[allow(clippy::float_cmp, clippy::expect_used)]
    fn test_predict_oom_imminent() {
        let history = [
            snap(1000, 10.0, 50.0),
            snap(2000, 10.0, 75.0),
        ];
        let pred = history.predict_trends(1000).expect("prediction failed");
        assert!(pred.is_oom_imminent);
        assert_eq!(pred.predicted_mem_pct, 100.0);
    }
}
