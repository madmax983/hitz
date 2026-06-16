//! Resource exhaustion forecasting.
//!
//! # Abstract
//! This module attempts to predict when a system will run out of memory
//! based on two recent `MetricsSnapshot` objects.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ResourceForecaster, ForecastResult, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let mut t1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let mut t2 = t1.clone();
//! t2.timestamp_ms = 2000;
//! t2.memory.used_bytes = 600;
//!
//! let forecast = t2.forecast_exhaustion(&t1);
//! assert!(matches!(forecast, ForecastResult::ExhaustionInSeconds(_)));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The result of forecasting resource exhaustion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ForecastResult {
    /// Exhaustion is projected to happen in the specified number of seconds.
    ExhaustionInSeconds(f64),
    /// The resource usage is stable or decreasing.
    Stable,
    /// Insufficient data to calculate a forecast (e.g., timestamps are identical).
    InsufficientData,
}

/// Trait to forecast resource exhaustion.
pub trait ResourceForecaster {
    /// Predicts time until memory exhaustion based on a previous snapshot.
    fn forecast_exhaustion(&self, previous: &Self) -> ForecastResult;
}

impl ResourceForecaster for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn forecast_exhaustion(&self, previous: &Self) -> ForecastResult {
        if self.timestamp_ms <= previous.timestamp_ms {
            return ForecastResult::InsufficientData;
        }

        let elapsed_secs = (self.timestamp_ms - previous.timestamp_ms) as f64 / 1000.0;

        // Ensure total_bytes is greater than 0
        if self.memory.total_bytes == 0 {
            return ForecastResult::InsufficientData;
        }

        let current_used = self.memory.used_bytes as f64;
        let previous_used = previous.memory.used_bytes as f64;

        if current_used <= previous_used {
            return ForecastResult::Stable;
        }

        let leak_rate_per_sec = (current_used - previous_used) / elapsed_secs;

        let remaining_bytes = self
            .memory
            .total_bytes
            .saturating_sub(self.memory.used_bytes) as f64;

        if leak_rate_per_sec > 0.0 {
            let seconds_to_exhaustion = remaining_bytes / leak_rate_per_sec;
            ForecastResult::ExhaustionInSeconds(seconds_to_exhaustion)
        } else {
            ForecastResult::Stable
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(time_ms: u64, total_mem: u64, used_mem: u64) -> MetricsSnapshot {
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
    #[allow(clippy::float_cmp)]
    fn test_steady_leak() {
        let t1 = dummy_snapshot(1000, 1000, 100);
        let t2 = dummy_snapshot(2000, 1000, 200); // Leaked 100 bytes over 1 second

        let forecast = t2.forecast_exhaustion(&t1);
        match forecast {
            ForecastResult::ExhaustionInSeconds(secs) => {
                // 800 bytes remaining, leaking at 100 bytes/sec -> 8 seconds
                assert_eq!(secs, 8.0);
            }
            _ => panic!("Expected ExhaustionInSeconds, got {forecast:?}"),
        }
    }

    #[test]
    fn test_stable_usage() {
        let t1 = dummy_snapshot(1000, 1000, 500);
        let t2 = dummy_snapshot(2000, 1000, 500);

        let forecast = t2.forecast_exhaustion(&t1);
        assert_eq!(forecast, ForecastResult::Stable);
    }

    #[test]
    fn test_decreasing_usage() {
        let t1 = dummy_snapshot(1000, 1000, 800);
        let t2 = dummy_snapshot(2000, 1000, 500); // Freed 300 bytes

        let forecast = t2.forecast_exhaustion(&t1);
        assert_eq!(forecast, ForecastResult::Stable);
    }

    #[test]
    fn test_insufficient_data() {
        let t1 = dummy_snapshot(1000, 1000, 500);
        let t2 = dummy_snapshot(1000, 1000, 600); // Same timestamp

        let forecast = t2.forecast_exhaustion(&t1);
        assert_eq!(forecast, ForecastResult::InsufficientData);
    }

    #[test]
    fn test_zero_total_memory() {
        let t1 = dummy_snapshot(1000, 0, 0);
        let t2 = dummy_snapshot(2000, 0, 10);

        let forecast = t2.forecast_exhaustion(&t1);
        assert_eq!(forecast, ForecastResult::InsufficientData);
    }
}
