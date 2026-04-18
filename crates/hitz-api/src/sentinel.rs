//! Sentinel Rules Engine Definitions.
//!
//! # Abstract
//! This module defines the building blocks for the Sentinel Rules Engine, allowing
//! users to define configurable, threshold-based alerts (watchdogs) that evaluate
//! incoming telemetry (`MetricsSnapshot`) in real-time.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{SentinelRule, SentinelCondition, MetricTarget, ConditionOperator, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! // 1. Define a rule that triggers if CPU exceeds 90%
//! let rule = SentinelRule {
//!     name: "CPU Spike Detected".to_string(),
//!     condition: SentinelCondition {
//!         target: MetricTarget::CpuTotalPct,
//!         operator: ConditionOperator::GreaterThan,
//!         threshold: 90.0,
//!     },
//! };
//!
//! // 2. Receive a snapshot from a struggling VM
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 0,
//!     cpu: CpuMetrics {
//!         total_pct: 95.0,
//!         per_core: vec![],
//!         load_avg: [0.0, 0.0, 0.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 0,
//!         used_bytes: 0,
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
//! // 3. The engine evaluates the snapshot and fires the alert!
//! assert!(rule.evaluate(&snap));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Operator used to evaluate a telemetry target against a threshold.
///
/// # Abstract
/// Defines the comparative logic for a sentinel watchdog condition.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::ConditionOperator;
///
/// let op = ConditionOperator::GreaterThan;
/// assert_eq!(op, ConditionOperator::GreaterThan);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionOperator {
    /// The metric must strictly exceed the threshold.
    GreaterThan,
    /// The metric must be strictly below the threshold.
    LessThan,
    /// The metric must perfectly match the threshold (within floating-point epsilon).
    Equals,
}

/// Target metric to inspect from the telemetry snapshot.
///
/// # Abstract
/// Specifies exactly which slice of a `MetricsSnapshot` should be evaluated.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::MetricTarget;
///
/// let target = MetricTarget::CpuTotalPct;
/// assert_eq!(target, MetricTarget::CpuTotalPct);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricTarget {
    /// Total CPU utilization percentage.
    CpuTotalPct,
    /// Total memory actively used in bytes.
    MemoryUsedBytes,
}

/// A single logical condition evaluated against live telemetry.
///
/// # Abstract
/// Combines a target metric, an operator, and a threshold into a single boolean assertion.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{SentinelCondition, MetricTarget, ConditionOperator};
///
/// let condition = SentinelCondition {
///     target: MetricTarget::CpuTotalPct,
///     operator: ConditionOperator::GreaterThan,
///     threshold: 90.0,
/// };
///
/// assert_eq!(condition.threshold, 90.0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentinelCondition {
    /// The telemetry field to evaluate.
    pub target: MetricTarget,
    /// The comparison operator.
    pub operator: ConditionOperator,
    /// The threshold value to compare against.
    pub threshold: f32,
}

/// A named rule used by the watchdog to trigger alerts or actions.
///
/// # Abstract
/// The fundamental unit of the Sentinel Rules Engine. Evaluates incoming `MetricsSnapshot`
/// payloads against predefined logic to spot anomalies (like CPU spikes or memory leaks).
///
/// # The Hero's Journey
/// ```rust
/// use hitz_api::{SentinelRule, SentinelCondition, MetricTarget, ConditionOperator, MetricsSnapshot, CpuMetrics, MemoryMetrics};
///
/// let rule = SentinelRule {
///     name: "CPU Spike Detected".to_string(),
///     condition: SentinelCondition {
///         target: MetricTarget::CpuTotalPct,
///         operator: ConditionOperator::GreaterThan,
///         threshold: 90.0,
///     },
/// };
///
/// let snap = MetricsSnapshot {
///     timestamp_ms: 0,
///     cpu: CpuMetrics {
///         total_pct: 95.0,
///         per_core: vec![],
///         load_avg: [0.0, 0.0, 0.0],
///     },
///     memory: MemoryMetrics {
///         total_bytes: 0,
///         used_bytes: 0,
///         free_bytes: 0,
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
/// assert!(rule.evaluate(&snap));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentinelRule {
    /// Human-readable name for this rule.
    pub name: String,
    /// The condition logic to evaluate.
    pub condition: SentinelCondition,
}

impl SentinelRule {
    /// Evaluates the underlying condition against a live telemetry snapshot.
    #[must_use]
    pub fn evaluate(&self, snapshot: &MetricsSnapshot) -> bool {
        let value = match self.condition.target {
            MetricTarget::CpuTotalPct => snapshot.cpu.total_pct,
            #[allow(clippy::cast_precision_loss)]
            MetricTarget::MemoryUsedBytes => snapshot.memory.used_bytes as f32,
        };

        match self.condition.operator {
            ConditionOperator::GreaterThan => value > self.condition.threshold,
            ConditionOperator::LessThan => value < self.condition.threshold,
            ConditionOperator::Equals => (value - self.condition.threshold).abs() < f32::EPSILON,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_sentinel_rule_evaluate() {
        let rule = SentinelRule {
            name: "High CPU".to_string(),
            condition: SentinelCondition {
                target: MetricTarget::CpuTotalPct,
                operator: ConditionOperator::GreaterThan,
                threshold: 80.0,
            },
        };

        let mut snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 85.0,
                per_core: vec![],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: 0,
                used_bytes: 0,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        assert!(rule.evaluate(&snap));

        snap.cpu.total_pct = 70.0;
        assert!(!rule.evaluate(&snap));
    }
}
