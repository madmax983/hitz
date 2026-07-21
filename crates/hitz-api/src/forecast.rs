//! Predictive analysis for VM metrics.
//!
//! # Abstract
//! This module provides a simple linear extrapolation tool for VM resource usage.
//! It allows consumers to predict future memory or CPU utilization based on
//! historical `MetricsSnapshot` sequences.

use crate::MetricsSnapshot;

/// Represents a predicted resource value at a specific future timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastResult {
    /// The extrapolated value.
    pub predicted_value: f64,
    /// A basic confidence score (0.0 to 1.0) based on sample size.
    pub confidence: f64,
}

/// Trait for objects (usually time-series sequences) that can predict future metrics.
pub trait ResourceForecaster {
    /// Predicts CPU utilization at `future_timestamp_ms`.
    fn predict_cpu(&self, future_timestamp_ms: u64) -> Option<ForecastResult>;
    /// Predicts memory utilization at `future_timestamp_ms`.
    fn predict_memory(&self, future_timestamp_ms: u64) -> Option<ForecastResult>;
}

/// Helper function to perform a simple linear regression (least squares).
/// Returns `(slope, intercept)` or `None` if unable to compute (e.g. insufficient data).
#[allow(
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn linear_regression(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if n < 2.0 {
        return None;
    }

    let sum_x: f64 = points.iter().map(|p| p.0).sum();
    let sum_y: f64 = points.iter().map(|p| p.1).sum();
    let sum_xy: f64 = points.iter().map(|p| p.0 * p.1).sum();
    let sum_x2: f64 = points.iter().map(|p| p.0 * p.0).sum();

    let denominator = n.mul_add(sum_x2, -(sum_x * sum_x));
    if denominator.abs() < f64::EPSILON {
        return None; // Vertical line or identical X coordinates
    }

    let slope = n.mul_add(sum_xy, -(sum_x * sum_y)) / denominator;
    let intercept = slope.mul_add(-sum_x, sum_y) / n;

    Some((slope, intercept))
}

impl ResourceForecaster for &[MetricsSnapshot] {
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn predict_cpu(&self, future_timestamp_ms: u64) -> Option<ForecastResult> {
        let points: Vec<(f64, f64)> = self
            .iter()
            .map(|s| (s.timestamp_ms as f64, f64::from(s.cpu.total_pct)))
            .collect();

        if let Some((slope, intercept)) = linear_regression(&points) {
            let predicted = slope.mul_add(future_timestamp_ms as f64, intercept);
            // Simple confidence: more data points = higher confidence, maxing out at 1.0 for >= 10 points.
            let confidence = ((self.len() as f64) / 10.0).min(1.0);
            Some(ForecastResult {
                predicted_value: predicted,
                confidence,
            })
        } else {
            None
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn predict_memory(&self, future_timestamp_ms: u64) -> Option<ForecastResult> {
        let points: Vec<(f64, f64)> = self
            .iter()
            .map(|s| (s.timestamp_ms as f64, s.memory.used_bytes as f64))
            .collect();

        if let Some((slope, intercept)) = linear_regression(&points) {
            let predicted = slope.mul_add(future_timestamp_ms as f64, intercept);
            // Simple confidence: more data points = higher confidence, maxing out at 1.0 for >= 10 points.
            let confidence = ((self.len() as f64) / 10.0).min(1.0);
            Some(ForecastResult {
                predicted_value: predicted,
                confidence,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(ts: u64, cpu: f32, ram: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: ts,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: ram,
                free_bytes: 1000 - ram,
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
    fn test_predict_memory_trend() {
        let history = vec![
            dummy_snapshot(1000, 10.0, 100),
            dummy_snapshot(2000, 20.0, 200),
            dummy_snapshot(3000, 30.0, 300),
        ];

        let forecast = history.as_slice().predict_memory(4000).unwrap();
        assert!((forecast.predicted_value - 400.0).abs() < f64::EPSILON);
        assert!((forecast.confidence - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_predict_cpu_trend() {
        let history = vec![
            dummy_snapshot(100, 10.0, 100),
            dummy_snapshot(200, 25.0, 200),
            dummy_snapshot(300, 40.0, 300),
        ];

        let forecast = history.as_slice().predict_cpu(400).unwrap();
        assert!((forecast.predicted_value - 55.0).abs() < 1e-10);
    }

    #[test]
    fn test_insufficient_data() {
        let history = vec![dummy_snapshot(100, 10.0, 100)];
        assert!(history.as_slice().predict_cpu(200).is_none());
    }
}
