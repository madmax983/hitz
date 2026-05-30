//! Imbalance scoring module for evaluating per-core CPU utilization imbalance.
//!
//! # Abstract
//! Calculates a workload imbalance score based on the per-core CPU utilization
//! of a VM. High imbalance suggests single-threaded bottlenecks in an otherwise
//! idle multi-core VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CoreImbalanceAnalyzer, ImbalanceResult, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 25.0,
//!         per_core: vec![100.0, 0.0, 0.0, 0.0],
//!         load_avg: [1.0, 0.5, 0.2],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 512 * 1024 * 1024,
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
//! let result = snap.analyze_imbalance();
//! assert!(result.is_imbalanced);
//! assert!(result.imbalance_score > 0.5);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// The computed resource imbalance score.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::ImbalanceResult;
///
/// let result = ImbalanceResult {
///     std_dev: 43.3,
///     imbalance_score: 0.85,
///     is_imbalanced: true,
/// };
///
/// assert!(result.is_imbalanced);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImbalanceResult {
    /// Overall standard deviation of per-core utilization.
    pub std_dev: f64,
    /// A normalized score representing the imbalance (0.0 to 1.0).
    pub imbalance_score: f64,
    /// Whether the workload is considered imbalanced (e.g.  > 0.5).
    pub is_imbalanced: bool,
}

/// Trait to analyze core utilization imbalance.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::{CoreImbalanceAnalyzer, ImbalanceResult, MetricsSnapshot, CpuMetrics, MemoryMetrics};
///
/// let snap = MetricsSnapshot {
///     timestamp_ms: 1000,
///     cpu: CpuMetrics {
///         total_pct: 25.0,
///         per_core: vec![100.0, 0.0, 0.0, 0.0],
///         load_avg: [1.0, 0.5, 0.2],
///     },
///     memory: MemoryMetrics {
///         total_bytes: 1024 * 1024 * 1024,
///         used_bytes: 512 * 1024 * 1024,
///         free_bytes: 512 * 1024 * 1024,
///         buffers_bytes: 0,
///         cached_bytes: 0,
///         swap_total: 0,
///         swap_used: 0,
///     },
///     disks: vec![],
///     networks: vec![],
///     processes: vec![],
/// };
///
/// let result = snap.analyze_imbalance();
/// assert!(result.is_imbalanced);
/// ```
pub trait CoreImbalanceAnalyzer {
    /// Analyzes per-core utilization and computes an imbalance score.
    fn analyze_imbalance(&self) -> ImbalanceResult;
}

impl CoreImbalanceAnalyzer for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn analyze_imbalance(&self) -> ImbalanceResult {
        let n = self.cpu.per_core.len();
        if n <= 1 {
            return ImbalanceResult {
                std_dev: 0.0,
                imbalance_score: 0.0,
                is_imbalanced: false,
            };
        }

        let n_f64 = n as f64;
        let sum: f64 = self.cpu.per_core.iter().map(|&c| f64::from(c)).sum();
        let mean = sum / n_f64;

        let variance = self
            .cpu
            .per_core
            .iter()
            .map(|&c| {
                let diff = f64::from(c) - mean;
                diff * diff
            })
            .sum::<f64>()
            / n_f64;

        let std_dev = variance.sqrt();

        // Max possible std dev occurs when one core is at 100% and the rest are 0% (or vice versa).
        // Let's compute a simple theoretical max for normalization depending on the mean.
        // If sum = 100 and n cores, mean = 100/n.
        // Variance = [ (100 - 100/n)^2 + (n-1)*(-100/n)^2 ] / n
        //          = [ 10000*(n-1)^2/n^2 + (n-1)*10000/n^2 ] / n
        //          = 10000 * (n-1)/n
        // max_std_dev = 100.0 * sqrt((n-1)/n)
        let max_std_dev = 100.0 * ((n_f64 - 1.0) / (n_f64 * n_f64)).sqrt();

        let imbalance_score = if max_std_dev > 0.0 {
            (std_dev / max_std_dev).clamp(0.0, 1.0)
        } else {
            0.0
        };

        ImbalanceResult {
            std_dev,
            imbalance_score,
            is_imbalanced: imbalance_score > 0.5,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(total: f32, per_core: Vec<f32>) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: total,
                per_core,
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
                free_bytes: 512 * 1024 * 1024,
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
    fn test_perfectly_balanced() {
        let snap = dummy_snapshot(50.0, vec![50.0, 50.0, 50.0, 50.0]);
        let result = snap.analyze_imbalance();
        assert_eq!(result.std_dev, 0.0);
        assert_eq!(result.imbalance_score, 0.0);
        assert!(!result.is_imbalanced);
    }

    #[test]
    fn test_zero_sum_mean() {
        let snap = dummy_snapshot(0.0, vec![0.0, 0.0, 0.0]);
        let result = snap.analyze_imbalance();
        assert_eq!(result.std_dev, 0.0);
        assert_eq!(result.imbalance_score, 0.0);
        assert!(!result.is_imbalanced);
    }
    #[test]
    fn test_highly_imbalanced() {
        // One core at 100%, three at 0%
        let snap = dummy_snapshot(25.0, vec![100.0, 0.0, 0.0, 0.0]);
        let result = snap.analyze_imbalance();
        assert!(result.std_dev > 40.0); // should be around 43.3
        assert!(result.imbalance_score > 0.8);
        assert!(result.is_imbalanced);
    }

    #[test]
    fn test_single_core_vm() {
        let snap = dummy_snapshot(100.0, vec![100.0]);
        let result = snap.analyze_imbalance();
        assert_eq!(result.std_dev, 0.0);
        assert_eq!(result.imbalance_score, 0.0);
        assert!(!result.is_imbalanced);
    }
}
