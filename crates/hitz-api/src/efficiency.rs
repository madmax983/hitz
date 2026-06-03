//! Efficiency scoring module.
//!
//! # Abstract
//! Calculates an efficiency score based on resource usage.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{EfficiencyScorer, EfficiencyScore, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 10.0,
//!         per_core: vec![10.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 4 * 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 0,
//!         buffers_bytes: 0,
//!         cached_bytes: 0,
//!         swap_total: 0,
//!         swap_used: 0,
//!     },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let score = snap.calculate_efficiency();
//! assert!(score.score < 50.0);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The computed resource efficiency score.
///
/// # Abstract
/// Represents the result of evaluating a system's resource usage, containing both a
/// numerical score and actionable insights.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::EfficiencyScore;
///
/// let result = EfficiencyScore {
///     score: 85.0,
///     insights: vec!["CPU is well utilized.".to_string()],
/// };
///
/// assert_eq!(result.score, 85.0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EfficiencyScore {
    /// Overall score from 0.0 to 100.0.
    pub score: f64,
    /// Detailed insights on resource waste or optimization opportunities.
    pub insights: Vec<String>,
}

/// Trait to calculate efficiency.
///
/// # Abstract
/// Provides an interface for evaluating how efficiently a system is utilizing
/// its allocated resources, returning a structured score and actionable insights.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{EfficiencyScorer, MetricsSnapshot, CpuMetrics, MemoryMetrics};
///
/// let snap = MetricsSnapshot {
///     timestamp_ms: 1000,
///     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
///     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
///     disks: vec![],
///     networks: vec![],
///     processes: vec![],
/// };
///
/// let score = snap.calculate_efficiency();
/// assert!(score.score < 100.0);
/// ```
pub trait EfficiencyScorer {
    /// Calculates efficiency from metrics.
    ///
    /// # Abstract
    /// Evaluates the current resource usage and returns an [`EfficiencyScore`].
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{EfficiencyScorer, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
    ///     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
    ///     disks: vec![],
    ///     networks: vec![],
    ///     processes: vec![],
    /// };
    ///
    /// let score = snap.calculate_efficiency();
    /// assert!(score.score < 100.0);
    /// ```
    fn calculate_efficiency(&self) -> EfficiencyScore;
}

impl EfficiencyScorer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_efficiency(&self) -> EfficiencyScore {
        let mut score = 100.0;
        let mut insights = Vec::with_capacity(2);

        // CPU evaluation
        // Optimal is considered > 40% usage. Less than that wastes CPU cycles.
        let cpu_usage = f64::from(self.cpu.total_pct);
        if cpu_usage < 40.0 {
            // Penalize score up to 50 points based on how idle the CPU is.
            let penalty = (40.0 - cpu_usage) * 1.25; // 50.0 / 40.0
            score -= penalty;
            insights.push(format!(
                "CPU underutilized: {cpu_usage:.1}%. Consider reducing vCPU count."
            ));
        }

        // Memory evaluation
        // Optimal is considered > 50% usage.
        if self.memory.total_bytes > 0 {
            let mem_usage_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            if mem_usage_pct < 50.0 {
                // Penalize score up to 50 points based on how idle the Memory is.
                let penalty = 50.0 - mem_usage_pct; // (50.0 / 50.0)
                score -= penalty;
                insights.push(format!(
                    "Memory underutilized: {mem_usage_pct:.1}%. Consider reducing allocated RAM."
                ));
            }
        }

        EfficiencyScore {
            score: score.clamp(0.0, 100.0),
            insights,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(cpu: f32, mem_total: u64, mem_used: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu,
                per_core: vec![cpu],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total,
                used_bytes: mem_used,
                free_bytes: mem_total.saturating_sub(mem_used),
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
    fn test_zero_memory_total() {
        let snap = dummy_snapshot(90.0, 0, 0);
        let score = snap.calculate_efficiency();
        assert_eq!(score.score, 100.0);
        assert!(score.insights.is_empty());
    }
    #[test]
    fn test_highly_efficient() {
        // High CPU, High Memory usage
        let snap = dummy_snapshot(90.0, 1024 * 1024 * 1024, 900 * 1024 * 1024);
        let score = snap.calculate_efficiency();
        assert!(score.score > 85.0);
        assert!(score.insights.is_empty());
    }

    #[test]
    fn test_underutilized() {
        // Low CPU, Low Memory usage
        let snap = dummy_snapshot(5.0, 4 * 1024 * 1024 * 1024, 100 * 1024 * 1024); // ~2.4% mem usage
        let score = snap.calculate_efficiency();
        assert!(score.score < 50.0);
        assert!(!score.insights.is_empty());
        assert!(score.insights.iter().any(|i| i.contains("CPU")));
        assert!(score.insights.iter().any(|i| i.contains("Memory")));
    }
}
