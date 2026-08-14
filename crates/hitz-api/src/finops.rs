#![allow(
    clippy::unreadable_literal,
    clippy::cast_precision_loss,
    clippy::float_cmp
)]

//! FinOps and Cost Estimation Module.
//!
//! # Abstract
//! This module provides a `CostEstimator` trait that connects the static configuration (`VmConfig`) or dynamic utilization (`MetricsSnapshot`) to calculate hourly or monthly costs based on a `PricingModel`.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{PricingModel, CostEstimator, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let model = PricingModel { per_vcpu_hour: 0.05, per_gb_ram_hour: 0.01 };
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let cost = config.estimate_hourly_cost(&model);
//! assert!(cost > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// A simple pricing model defining unit economics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Cost per vCPU per hour in USD.
    pub per_vcpu_hour: f64,
    /// Cost per GB of RAM per hour in USD.
    pub per_gb_ram_hour: f64,
}

impl PricingModel {
    /// Creates a new `PricingModel` with default standard rates.
    #[must_use]
    pub const fn new(vcpu: f64, ram: f64) -> Self {
        Self {
            per_vcpu_hour: vcpu,
            per_gb_ram_hour: ram,
        }
    }
}

/// Trait for estimating financial cost of a resource.
pub trait CostEstimator {
    /// Estimates the hourly cost in USD.
    fn estimate_hourly_cost(&self, model: &PricingModel) -> f64;
    /// Estimates the monthly cost (assuming 730 hours/month) in USD.
    fn estimate_monthly_cost(&self, model: &PricingModel) -> f64 {
        self.estimate_hourly_cost(model) * 730.0
    }
}

impl CostEstimator for VmConfig {
    fn estimate_hourly_cost(&self, model: &PricingModel) -> f64 {
        let cpu_cost = f64::from(self.cpus) * model.per_vcpu_hour;
        let ram_gb = f64::from(self.ram_mib) / 1024.0;
        let ram_cost = ram_gb * model.per_gb_ram_hour;
        cpu_cost + ram_cost
    }
}

impl CostEstimator for MetricsSnapshot {
    fn estimate_hourly_cost(&self, model: &PricingModel) -> f64 {
        let cpu_usage_pct = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        // Since we don't know max vcpus here, we estimate based on utilization.
        // Assuming 1 vCPU equivalent for this metric.
        let cpu_cost = cpu_usage_pct * model.per_vcpu_hour;
        let ram_gb = (self.memory.used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let ram_cost = ram_gb * model.per_gb_ram_hour;
        cpu_cost + ram_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_static_cost() {
        let model = PricingModel::new(0.04, 0.005);
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 4096,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: crate::GuestAgentMode::Auto,
        };
        let hourly = config.estimate_hourly_cost(&model);
        assert!((hourly - (0.08 + 0.02)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_dynamic_cost() {
        let model = PricingModel::new(0.04, 0.005);
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 4294967296,
                used_bytes: 2147483648, // 2GB
                free_bytes: 2147483648,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let hourly = snap.estimate_hourly_cost(&model);
        let expected = 0.5f64.mul_add(0.04, 2.0 * 0.005);
        assert!((hourly - expected).abs() < f64::EPSILON);
    }
}
