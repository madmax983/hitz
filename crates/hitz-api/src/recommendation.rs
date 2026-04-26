//! Rightsizing recommendation engine for micro-VMs.
//!
//! # Abstract
//! This module analyzes real-time `MetricsSnapshot` telemetry to provide
//! actionable recommendations for scaling VM resources up or down. It helps
//! users optimize for cost (downsizing) and performance (upsizing).
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{RecommendationEngine, ResizeRecommendation, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 5.0, // Very low CPU
//!         per_core: vec![5.0, 5.0, 5.0, 5.0], // 4 cores barely used
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 16 * 1024 * 1024 * 1024, // 16 GB
//!         used_bytes: 1 * 1024 * 1024 * 1024, // 1 GB used
//!         free_bytes: 15 * 1024 * 1024 * 1024,
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
//! let rec = snap.recommend_sizing();
//! assert_eq!(rec.cpu, ResizeRecommendation::Downsize);
//! assert_eq!(rec.memory, ResizeRecommendation::Downsize);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Recommendation for a specific resource dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResizeRecommendation {
    /// Resource is underutilized, consider downsizing to save cost.
    Downsize,
    /// Resource is optimally utilized.
    Keep,
    /// Resource is overutilized, consider upsizing for better performance.
    Upsize,
}

/// A set of rightsizing recommendations for a VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmRecommendation {
    /// Recommendation for CPU cores.
    pub cpu: ResizeRecommendation,
    /// Recommendation for RAM allocation.
    pub memory: ResizeRecommendation,
    /// A human-readable summary of the recommendation.
    pub reason: String,
}

/// Trait to generate rightsizing recommendations from telemetry.
pub trait RecommendationEngine {
    /// Evaluates the resource usage and suggests scaling actions.
    #[must_use]
    fn recommend_sizing(&self) -> VmRecommendation;
}

impl RecommendationEngine for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn recommend_sizing(&self) -> VmRecommendation {
        let cpu_rec = if self.cpu.total_pct > 85.0 {
            ResizeRecommendation::Upsize
        } else if self.cpu.total_pct < 15.0 && self.cpu.per_core.len() > 1 {
            ResizeRecommendation::Downsize
        } else {
            ResizeRecommendation::Keep
        };

        // Guard against division by zero if total_bytes is somehow 0
        let mem_usage_pct = if self.memory.total_bytes > 0 {
            (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let mem_rec = if mem_usage_pct > 90.0 || self.memory.swap_used > 0 {
            ResizeRecommendation::Upsize
        } else if mem_usage_pct < 20.0 && self.memory.total_bytes > 512 * 1024 * 1024 {
            ResizeRecommendation::Downsize
        } else {
            ResizeRecommendation::Keep
        };

        let mut reasons = Vec::new();
        if cpu_rec == ResizeRecommendation::Downsize {
            reasons.push("CPU is underutilized (<15%)");
        } else if cpu_rec == ResizeRecommendation::Upsize {
            reasons.push("CPU is bottlenecking (>85%)");
        }

        if mem_rec == ResizeRecommendation::Downsize {
            reasons.push("Memory is underutilized (<20%)");
        } else if mem_rec == ResizeRecommendation::Upsize {
            if self.memory.swap_used > 0 {
                reasons.push("Memory is swapping");
            } else {
                reasons.push("Memory is heavily utilized (>90%)");
            }
        }

        let reason = if reasons.is_empty() {
            "Resources are optimally provisioned.".to_string()
        } else {
            reasons.join(", ")
        };

        VmRecommendation {
            cpu: cpu_rec,
            memory: mem_rec,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn dummy_snapshot(
        cpu_pct: f32,
        cpu_cores: usize,
        mem_used_mb: u64,
        mem_total_mb: u64,
        swap_used: u64,
    ) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct; cpu_cores],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: mem_total_mb * 1024 * 1024,
                used_bytes: mem_used_mb * 1024 * 1024,
                free_bytes: (mem_total_mb - mem_used_mb) * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: swap_used, // Just for test
                swap_used,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        }
    }

    #[test]
    fn test_recommend_downsize() {
        // 5% CPU on 4 cores, 1GB used out of 16GB
        let snap = dummy_snapshot(5.0, 4, 1024, 16384, 0);
        let rec = snap.recommend_sizing();
        assert_eq!(rec.cpu, ResizeRecommendation::Downsize);
        assert_eq!(rec.memory, ResizeRecommendation::Downsize);
        assert!(rec.reason.contains("CPU is underutilized"));
        assert!(rec.reason.contains("Memory is underutilized"));
    }

    #[test]
    fn test_recommend_upsize() {
        // 90% CPU, 15GB used out of 16GB
        let snap = dummy_snapshot(90.0, 4, 15000, 16384, 0);
        let rec = snap.recommend_sizing();
        assert_eq!(rec.cpu, ResizeRecommendation::Upsize);
        assert_eq!(rec.memory, ResizeRecommendation::Upsize);
        assert!(rec.reason.contains("CPU is bottlenecking"));
        assert!(rec.reason.contains("Memory is heavily utilized"));
    }

    #[test]
    fn test_recommend_keep() {
        // 50% CPU, 8GB used out of 16GB
        let snap = dummy_snapshot(50.0, 4, 8192, 16384, 0);
        let rec = snap.recommend_sizing();
        assert_eq!(rec.cpu, ResizeRecommendation::Keep);
        assert_eq!(rec.memory, ResizeRecommendation::Keep);
        assert_eq!(rec.reason, "Resources are optimally provisioned.");
    }

    #[test]
    fn test_swap_upsize() {
        // 50% CPU, 8GB used out of 16GB, but swapping!
        let snap = dummy_snapshot(50.0, 4, 8192, 16384, 1024);
        let rec = snap.recommend_sizing();
        assert_eq!(rec.cpu, ResizeRecommendation::Keep);
        assert_eq!(rec.memory, ResizeRecommendation::Upsize);
        assert!(rec.reason.contains("Memory is swapping"));
    }
}
