//! Cloud FinOps Cost Simulator for micro-VMs.
//!
//! # Abstract
//! This module provides a `BillingEstimator` trait that connects absolute
//! resource utilization (`MetricsSnapshot`) with configurable serverless
//! pricing models to estimate the real-time cost run-rate of the VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{BillingEstimator, ServerlessPricingModel, MetricsSnapshot};
//!
//! // Assume we have a snapshot showing 50% CPU usage
//! // let snap: MetricsSnapshot = ...;
//! //
//! // // Cloud pricing: $0.04/vCPU/hour, $0.005/GB RAM/hour
//! // let pricing = ServerlessPricingModel::new(0.04, 0.005);
//! // let run_rate = snap.estimate_run_rate(&pricing, 4); // 4 CPUs
//! // println!("Estimated cost: ${}/hour", run_rate.hourly_cost_usd);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable serverless pricing model.
///
/// # Abstract
/// This struct holds the per-unit costs used to translate compute and memory
/// utilization into USD run-rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerlessPricingModel {
    /// Cost per active vCPU per hour in USD.
    pub vcpu_hourly_usd: f64,
    /// Cost per actively used GB of RAM per hour in USD.
    pub ram_gb_hourly_usd: f64,
}

impl ServerlessPricingModel {
    /// Create a new pricing model with the given hourly rates.
    #[must_use]
    pub const fn new(vcpu_hourly_usd: f64, ram_gb_hourly_usd: f64) -> Self {
        Self {
            vcpu_hourly_usd,
            ram_gb_hourly_usd,
        }
    }
}

/// Estimated cost run-rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRate {
    /// Projected hourly cost in USD based on current utilization.
    pub hourly_cost_usd: f64,
}

/// Trait for estimating billing run-rates from a metrics snapshot.
pub trait BillingEstimator {
    /// Estimate the current billing run-rate using the provided pricing model
    /// and total allocated CPUs.
    fn estimate_run_rate(&self, pricing: &ServerlessPricingModel, total_cpus: u32) -> RunRate;
}

impl BillingEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_run_rate(&self, pricing: &ServerlessPricingModel, total_cpus: u32) -> RunRate {
        let active_cpu_cores = f64::from(total_cpus) * (f64::from(self.cpu.total_pct) / 100.0);
        let active_ram_gb = self.memory.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        let vcpu_cost = active_cpu_cores * pricing.vcpu_hourly_usd;
        let ram_cost = active_ram_gb * pricing.ram_gb_hourly_usd;

        RunRate {
            hourly_cost_usd: vcpu_cost + ram_cost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_estimate_run_rate() {
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![100.0, 0.0],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: 4 * 1024 * 1024 * 1024,
                used_bytes: 2 * 1024 * 1024 * 1024,
                free_bytes: 2 * 1024 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        // $1.00 per vCPU, $0.50 per GB RAM
        let pricing = ServerlessPricingModel::new(1.0, 0.5);
        let run_rate = snap.estimate_run_rate(&pricing, 2);

        // Active CPUs: 50% of 2 = 1 CPU. Cost = 1 * $1.00 = $1.00
        // Active RAM: 2 GB. Cost = 2 * $0.50 = $1.00
        // Total expected: $2.00
        assert!((run_rate.hourly_cost_usd - 2.0).abs() < f64::EPSILON);
    }
}
