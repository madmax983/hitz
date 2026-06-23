//! Telemetry Anomaly Detection.
//!
//! # Abstract
//! This module provides an `AnomalyDetector` that tracks the Exponential Moving Average
//! (EMA) and variance of CPU and Memory usage to detect sudden spikes or abnormal behavior
//! in a stream of `MetricsSnapshot`s.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{AnomalyDetector, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let mut detector = AnomalyDetector::new(0.1, 3.0); // alpha 0.1, threshold 3 sigmas
//!
//! let snap_normal = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! // Train with normal data
//! for _ in 0..10 {
//!     let is_anomaly = detector.observe(&snap_normal);
//!     assert!(!is_anomaly);
//! }
//!
//! // Introduce a spike
//! let snap_spike = MetricsSnapshot {
//!     cpu: CpuMetrics { total_pct: 95.0, per_core: vec![95.0], load_avg: [2.0, 1.0, 1.0] },
//!     ..snap_normal
//! };
//!
//! let is_anomaly = detector.observe(&snap_spike);
//! assert!(is_anomaly);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Tracks EMA and variance to detect anomalies in a single metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricTracker {
    ema: f64,
    ema_variance: f64,
    alpha: f64,
    initialized: bool,
}

impl MetricTracker {
    /// Creates a new metric tracker with the given alpha (smoothing factor).
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        Self {
            ema: 0.0,
            ema_variance: 0.0,
            alpha,
            initialized: false,
        }
    }

    /// Observes a new value and returns true if it exceeds the Z-score threshold.
    pub fn observe(&mut self, value: f64, z_threshold: f64) -> bool {
        if !self.initialized {
            self.ema = value;
            self.ema_variance = 0.0;
            self.initialized = true;
            return false;
        }

        let diff = value - self.ema;
        let is_anomaly = if self.ema_variance > 0.0 {
            let std_dev = self.ema_variance.sqrt();
            let z_score = diff.abs() / std_dev;
            z_score > z_threshold
        } else {
            // If variance is 0, any non-zero difference is technically an anomaly,
            // but we'll wait for variance to build up unless the difference is large.
            diff.abs() > 1.0
        };

        // Update EMA and Variance
        let incr = self.alpha * diff;
        self.ema += incr;
        // Exponential moving variance update
        self.ema_variance = (1.0 - self.alpha) * diff.mul_add(incr, self.ema_variance);

        is_anomaly
    }
}

/// Detects anomalies across multiple metrics in a VM snapshot stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnomalyDetector {
    /// Tracks overall CPU percentage.
    pub cpu_tracker: MetricTracker,
    /// Tracks memory usage percentage.
    pub mem_tracker: MetricTracker,
    /// Number of standard deviations to trigger an anomaly.
    pub z_threshold: f64,
}

impl AnomalyDetector {
    /// Creates a new anomaly detector.
    ///
    /// * `alpha`: The smoothing factor for EMA (e.g., 0.1).
    /// * `z_threshold`: The Z-score threshold for anomaly detection (e.g., 3.0 for 3 sigma).
    #[must_use]
    pub const fn new(alpha: f64, z_threshold: f64) -> Self {
        Self {
            cpu_tracker: MetricTracker::new(alpha),
            mem_tracker: MetricTracker::new(alpha),
            z_threshold,
        }
    }

    /// Observes a new snapshot and returns true if any core metric is anomalous.
    pub fn observe(&mut self, snapshot: &MetricsSnapshot) -> bool {
        let cpu_val = f64::from(snapshot.cpu.total_pct);
        let cpu_anomaly = self.cpu_tracker.observe(cpu_val, self.z_threshold);

        let mem_anomaly = if snapshot.memory.total_bytes > 0 {
            #[allow(clippy::cast_precision_loss)]
            let mem_pct =
                (snapshot.memory.used_bytes as f64 / snapshot.memory.total_bytes as f64) * 100.0;
            self.mem_tracker.observe(mem_pct, self.z_threshold)
        } else {
            false
        };

        cpu_anomaly || mem_anomaly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn create_snapshot(cpu: f32, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: mem_used,
                free_bytes: 1000 - mem_used,
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
    fn test_anomaly_detection_cpu_spike() {
        let mut detector = AnomalyDetector::new(0.2, 2.5);

        // Train with low CPU usage
        for _ in 0..20 {
            let snap = create_snapshot(10.0, 100);
            assert!(!detector.observe(&snap));
        }

        // Introduce a sudden CPU spike
        let spike = create_snapshot(95.0, 100);
        assert!(detector.observe(&spike));
    }

    #[test]
    fn test_anomaly_detection_memory_spike() {
        let mut detector = AnomalyDetector::new(0.2, 2.5);

        // Train with normal memory
        for _ in 0..20 {
            let snap = create_snapshot(5.0, 200);
            assert!(!detector.observe(&snap));
        }

        // Introduce a sudden Memory spike
        let spike = create_snapshot(5.0, 900);
        assert!(detector.observe(&spike));
    }
}
