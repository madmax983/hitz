//! OOM (Out Of Memory) Prediction module.
//!
//! # Abstract
//! Uses metrics diffs to estimate when a VM will run out of memory.

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The output of the OOM predictor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OomPrediction {
    /// Whether an OOM is currently predicted within a threshold.
    pub is_predicted: bool,
    /// Estimated time until OOM in seconds. None if not growing.
    pub time_to_oom_secs: Option<f64>,
}

/// Trait to predict OOM events based on current usage and rate of change.
pub trait OomPredictor {
    /// Evaluates if an OOM is imminent based on a previous snapshot.
    fn predict_oom(&self, previous: &MetricsSnapshot) -> OomPrediction;
}

impl OomPredictor for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn predict_oom(&self, previous: &MetricsSnapshot) -> OomPrediction {
        if self.timestamp_ms <= previous.timestamp_ms {
            return OomPrediction {
                is_predicted: false,
                time_to_oom_secs: None,
            };
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;
        let mem_growth = self.memory.used_bytes as f64 - previous.memory.used_bytes as f64;

        if mem_growth <= 0.0 {
            return OomPrediction {
                is_predicted: false,
                time_to_oom_secs: None,
            };
        }

        let growth_rate_per_sec = mem_growth / elapsed_secs;
        let remaining_bytes = self
            .memory
            .total_bytes
            .saturating_sub(self.memory.used_bytes) as f64;

        let time_to_oom_secs = remaining_bytes / growth_rate_per_sec;

        OomPrediction {
            is_predicted: time_to_oom_secs < 60.0, // Alert if < 1 minute
            time_to_oom_secs: Some(time_to_oom_secs),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(time_ms: u64, used: u64, total: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: time_ms,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: total,
                used_bytes: used,
                free_bytes: total.saturating_sub(used),
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
        // T1: 500MB used, 1000MB total
        let t1 = dummy_snapshot(1000, 500 * 1024 * 1024, 1000 * 1024 * 1024);

        // T2 (1 sec later): 600MB used, 1000MB total.
        // Growth rate: 100MB/s. Remaining: 400MB. OOM in 4 seconds.
        let t2 = dummy_snapshot(2000, 600 * 1024 * 1024, 1000 * 1024 * 1024);

        let prediction = t2.predict_oom(&t1);

        assert!(prediction.is_predicted);
        assert_eq!(prediction.time_to_oom_secs, Some(4.0));
    }

    #[test]
    fn test_predict_oom_no_growth() {
        // T1: 500MB used, 1000MB total
        let t1 = dummy_snapshot(1000, 500 * 1024 * 1024, 1000 * 1024 * 1024);

        // T2 (1 sec later): 500MB used, 1000MB total.
        let t2 = dummy_snapshot(2000, 500 * 1024 * 1024, 1000 * 1024 * 1024);

        let prediction = t2.predict_oom(&t1);

        assert!(!prediction.is_predicted);
        assert_eq!(prediction.time_to_oom_secs, None);
    }
}
