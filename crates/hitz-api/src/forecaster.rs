//! Resource exhaustion forecasting module.
//!
//! # Abstract
//! Predicts future resource exhaustion (like OOM) by analyzing historical metrics
//! using simple linear regression.

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Result of a forecasting operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastResult {
    /// True if exhaustion is predicted.
    pub will_exhaust: bool,
    /// Estimated milliseconds until exhaustion (if `will_exhaust` is true).
    pub time_to_exhaustion_ms: Option<u64>,
}

/// Forecaster for resource exhaustion.
#[derive(Debug, Default)]
pub struct ResourceForecaster {
    history: Vec<MetricsSnapshot>,
}

impl ResourceForecaster {
    /// Creates a new, empty forecaster.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    /// Adds a new metric snapshot to the history.
    pub fn record(&mut self, snapshot: MetricsSnapshot) {
        self.history.push(snapshot);
    }

    /// Predicts if and when memory will be exhausted based on historical trends.
    #[allow(clippy::missing_panics_doc)]
    #[allow(
        clippy::cast_precision_loss,
        clippy::suboptimal_flops,
        clippy::similar_names,
        clippy::suspicious_operation_groupings,
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[must_use]
    pub fn predict_oom(&self) -> ForecastResult {
        if self.history.len() < 2 {
            return ForecastResult {
                will_exhaust: false,
                time_to_exhaustion_ms: None,
            };
        }

        let n = self.history.len() as f64;

        let sum_x: f64 = self.history.iter().map(|s| s.timestamp_ms as f64).sum();
        let sum_y: f64 = self
            .history
            .iter()
            .map(|s| s.memory.used_bytes as f64)
            .sum();
        let sum_xy: f64 = self
            .history
            .iter()
            .map(|s| (s.timestamp_ms as f64) * (s.memory.used_bytes as f64))
            .sum();
        let sum_xx: f64 = self
            .history
            .iter()
            .map(|s| (s.timestamp_ms as f64) * (s.timestamp_ms as f64))
            .sum();

        let denominator = n * sum_xx - sum_x * sum_x;
        if denominator == 0.0 {
            return ForecastResult {
                will_exhaust: false,
                time_to_exhaustion_ms: None,
            };
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denominator;

        // If slope is not positive, memory usage is not strictly increasing.
        if slope <= 0.0 {
            return ForecastResult {
                will_exhaust: false,
                time_to_exhaustion_ms: None,
            };
        }

        let intercept = (sum_y - slope * sum_x) / n;

        // We assume total_bytes is constant and take the latest one.
        let last_snap = self.history.last().unwrap();
        let total_mem = last_snap.memory.total_bytes as f64;

        // We want to find x where y = total_mem
        // y = mx + b  =>  x = (y - b) / m
        let time_to_exhaust = (total_mem - intercept) / slope;
        let current_time = last_snap.timestamp_ms as f64;

        if time_to_exhaust < current_time {
            // It's already exhausted or the calculation is weird.
            return ForecastResult {
                will_exhaust: true,
                time_to_exhaustion_ms: Some(0),
            };
        }

        let ms_until = (time_to_exhaust - current_time) as u64;

        ForecastResult {
            will_exhaust: true,
            time_to_exhaustion_ms: Some(ms_until),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn make_snapshot(time_ms: u64, used_mem: u64, total_mem: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: time_ms,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: total_mem,
                used_bytes: used_mem,
                free_bytes: total_mem.saturating_sub(used_mem),
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
    fn test_predict_oom() {
        let mut forecaster = ResourceForecaster::new();
        let total_mem = 1000;

        // Steady leak: 100 units per 1000ms
        forecaster.record(make_snapshot(1000, 100, total_mem));
        forecaster.record(make_snapshot(2000, 200, total_mem));
        forecaster.record(make_snapshot(3000, 300, total_mem));

        let forecast = forecaster.predict_oom();

        assert!(forecast.will_exhaust, "Should predict OOM");

        // We start at t=0 with 0 used?
        // t=1000, used=100 -> slope is 100 units / 1000 ms = 0.1 units / ms
        // y = mx + b. We have:
        // 100 = 0.1 * 1000 + b -> b = 0.
        // We want to know when y = 1000.
        // 1000 = 0.1 * x -> x = 10000.
        // Current time is 3000.
        // Time to exhaustion = 10000 - 3000 = 7000.

        assert_eq!(forecast.time_to_exhaustion_ms, Some(7000));
    }
}
