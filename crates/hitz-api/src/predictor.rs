//! Resource Exhaustion Predictor Module
//!
//! # Abstract
//! Predicts the time remaining until critical resource exhaustion (like OOM)
//! by analyzing the rate of change between two telemetry snapshots.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{ExhaustionPredictor, TimeToExhaustion, MetricsSnapshot, MemoryMetrics, CpuMetrics};
//!
//! let snap1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let snap2 = MetricsSnapshot {
//!     timestamp_ms: 2000,
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 768, free_bytes: 256, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     cpu: CpuMetrics { total_pct: 15.0, per_core: vec![15.0], load_avg: [0.1, 0.1, 0.1] },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let prediction = snap1.predict_exhaustion(&snap2);
//! assert_eq!(prediction.memory_exhaustion_secs, Some(1.0));
//! ```

use crate::MetricsSnapshot;

/// Result of an exhaustion prediction.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeToExhaustion {
    /// Estimated time in seconds until memory is fully exhausted.
    /// Returns `None` if memory usage is not increasing.
    pub memory_exhaustion_secs: Option<f64>,
}

/// Trait to predict resource exhaustion based on historical telemetry.
pub trait ExhaustionPredictor {
    /// Predicts time to exhaustion by comparing an older snapshot (`self`)
    /// with a newer snapshot (`newer`).
    fn predict_exhaustion(&self, newer: &MetricsSnapshot) -> TimeToExhaustion;
}

impl ExhaustionPredictor for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn predict_exhaustion(&self, newer: &MetricsSnapshot) -> TimeToExhaustion {
        let elapsed_secs = (newer.timestamp_ms.saturating_sub(self.timestamp_ms)) as f64 / 1000.0;

        if elapsed_secs <= 0.0 {
            return TimeToExhaustion {
                memory_exhaustion_secs: None,
            };
        }

        let memory_diff = newer
            .memory
            .used_bytes
            .saturating_sub(self.memory.used_bytes) as f64;
        let memory_rate = memory_diff / elapsed_secs;

        let memory_exhaustion_secs = if memory_rate > 0.0 {
            let remaining_bytes = newer
                .memory
                .total_bytes
                .saturating_sub(newer.memory.used_bytes) as f64;
            Some(remaining_bytes / memory_rate)
        } else {
            None
        };

        TimeToExhaustion {
            memory_exhaustion_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_predict_exhaustion() {
        let snap1 = MetricsSnapshot {
            timestamp_ms: 1000,
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let snap2 = MetricsSnapshot {
            timestamp_ms: 2000,
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 768,
                free_bytes: 256,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            cpu: CpuMetrics {
                total_pct: 15.0,
                per_core: vec![15.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let prediction = snap1.predict_exhaustion(&snap2);
        assert_eq!(prediction.memory_exhaustion_secs, Some(1.0));
    }

    #[test]
    fn test_no_exhaustion() {
        let snap1 = MetricsSnapshot {
            timestamp_ms: 1000,
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let snap2 = MetricsSnapshot {
            timestamp_ms: 2000,
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 256,
                free_bytes: 768,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            cpu: CpuMetrics {
                total_pct: 15.0,
                per_core: vec![15.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let prediction = snap1.predict_exhaustion(&snap2);
        assert_eq!(prediction.memory_exhaustion_secs, None);
    }
}
