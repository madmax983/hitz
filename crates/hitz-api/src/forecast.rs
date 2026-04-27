//! Capacity Forecasting Module.
//!
//! # Abstract
//! This module introduces predictive analytics by analyzing telemetry snapshots
//! to forecast when a micro-VM will exhaust critical resources (Time to Exhaustion, or TTE).
//! It estimates when the VM might hit an Out-Of-Memory (OOM) panic or CPU saturation.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics, forecast_capacity};
//!
//! let t1 = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![], load_avg: [0.0; 3] },
//!     memory: MemoryMetrics { total_bytes: 1000, used_bytes: 500, free_bytes: 500, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let t2 = MetricsSnapshot {
//!     timestamp_ms: 2000, // 1 second later
//!     cpu: CpuMetrics { total_pct: 20.0, per_core: vec![], load_avg: [0.0; 3] }, // +10% per sec
//!     memory: MemoryMetrics { total_bytes: 1000, used_bytes: 600, free_bytes: 400, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 }, // +100 bytes per sec
//!     disks: vec![], networks: vec![], processes: vec![],
//! };
//!
//! let forecast = forecast_capacity(&t1, &t2);
//! assert_eq!(forecast.time_to_oom_ms, Some(4000)); // 400 bytes left / 100 bytes per sec = 4s
//! assert_eq!(forecast.time_to_cpu_saturation_ms, Some(8000)); // 80% left / 10% per sec = 8s
//! ```

use crate::MetricsSnapshot;

/// Forecasting results for critical resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityForecast {
    /// Estimated milliseconds until memory is fully exhausted. `None` if usage is stable or decreasing.
    pub time_to_oom_ms: Option<u64>,
    /// Estimated milliseconds until CPU reaches 100%. `None` if usage is stable or decreasing.
    pub time_to_cpu_saturation_ms: Option<u64>,
}

/// Forecasts the Time-to-Exhaustion (TTE) for CPU and Memory based on a linear trend between two snapshots.
#[must_use]
pub fn forecast_capacity(
    baseline: &MetricsSnapshot,
    current: &MetricsSnapshot,
) -> CapacityForecast {
    let time_delta_ms = current.timestamp_ms.saturating_sub(baseline.timestamp_ms);

    if time_delta_ms == 0 {
        return CapacityForecast {
            time_to_oom_ms: None,
            time_to_cpu_saturation_ms: None,
        };
    }

    // Memory forecast
    #[allow(clippy::cast_possible_wrap)]
    let mem_delta: i64 = (current.memory.used_bytes as i64) - (baseline.memory.used_bytes as i64);
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    let time_to_oom_ms = if mem_delta > 0 {
        let bytes_per_ms = (mem_delta as f64) / (time_delta_ms as f64);
        let remaining_bytes = current.memory.free_bytes as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_precision_loss, clippy::cast_lossless)]
        Some((remaining_bytes / bytes_per_ms) as u64)
    } else {
        None
    };

    // CPU forecast
    let cpu_delta = current.cpu.total_pct - baseline.cpu.total_pct;
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    let time_to_cpu_saturation_ms = if cpu_delta > 0.0 {
        let pct_per_ms = (cpu_delta as f64) / (time_delta_ms as f64);
        let remaining_pct = (100.0 - current.cpu.total_pct).max(0.0) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_precision_loss, clippy::cast_lossless)]
        Some((remaining_pct / pct_per_ms) as u64)
    } else {
        None
    };

    CapacityForecast {
        time_to_oom_ms,
        time_to_cpu_saturation_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_forecast_capacity_exhaustion() {
        let t1 = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 500,
                free_bytes: 500,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let t2 = MetricsSnapshot {
            timestamp_ms: 2000,
            cpu: CpuMetrics {
                total_pct: 20.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 600,
                free_bytes: 400,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let forecast = forecast_capacity(&t1, &t2);
        assert_eq!(forecast.time_to_oom_ms, Some(4000));
        assert_eq!(forecast.time_to_cpu_saturation_ms, Some(8000));
    }

    #[test]
    fn test_forecast_capacity_stable() {
        let t1 = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 500,
                free_bytes: 500,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let t2 = MetricsSnapshot {
            timestamp_ms: 2000,
            cpu: CpuMetrics {
                total_pct: 40.0,
                per_core: vec![],
                load_avg: [0.0; 3],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 400,
                free_bytes: 600,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let forecast = forecast_capacity(&t1, &t2);
        assert_eq!(forecast.time_to_oom_ms, None);
        assert_eq!(forecast.time_to_cpu_saturation_ms, None);
    }
}
