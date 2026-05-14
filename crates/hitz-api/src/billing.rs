//! Cloud Cost and Billing Estimator for micro-VMs.
//!
//! # Abstract
//! This module translates hardware configurations and telemetry usage into
//! financial cost estimates based on a pricing model.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{CostEstimator, PricingModel, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, NetMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
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
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
//!     memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
//!     disks: vec![],
//!     networks: vec![
//!         NetMetrics {
//!             interface: "eth0".to_string(),
//!             rx_bytes: 0,
//!             tx_bytes: 1073741824, // 1 GB
//!             rx_packets: 0,
//!             tx_packets: 0,
//!             rx_errors: 0,
//!             tx_errors: 0,
//!         }
//!     ],
//!     processes: vec![],
//! };
//!
//! let model = PricingModel {
//!     cpu_hourly_rate: 0.05,
//!     ram_gb_hourly_rate: 0.01,
//!     network_egress_gb_rate: 0.08,
//! };
//!
//! let cost = snap.estimate_hourly_cost(&config, &model);
//! assert!(cost > 0.0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Pricing model defining rates for various resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingModel {
    /// Hourly cost per vCPU.
    pub cpu_hourly_rate: f64,
    /// Hourly cost per GB of RAM.
    pub ram_gb_hourly_rate: f64,
    /// Cost per GB of network egress.
    pub network_egress_gb_rate: f64,
}

/// Trait for estimating hourly running costs.
pub trait CostEstimator {
    /// Estimates the hourly cost based on config and current metrics.
    fn estimate_hourly_cost(&self, config: &VmConfig, model: &PricingModel) -> f64;
}

impl CostEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_hourly_cost(&self, config: &VmConfig, model: &PricingModel) -> f64 {
        let cpu_cost = f64::from(config.cpus) * model.cpu_hourly_rate;
        let ram_gb = f64::from(config.ram_mib) / 1024.0;
        let compute_cost = ram_gb.mul_add(model.ram_gb_hourly_rate, cpu_cost);

        let total_tx_bytes: u64 = self.networks.iter().map(|n| n.tx_bytes).sum();
        let tx_gb = total_tx_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        tx_gb.mul_add(model.network_egress_gb_rate, compute_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics, NetMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_cost_calculation() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 2048,
            cpus: 2,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 0,
                tx_bytes: 1_073_741_824 * 5, // 5 GB
                rx_packets: 0,
                tx_packets: 0,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        };

        let model = PricingModel {
            cpu_hourly_rate: 0.10,        // 2 vCPUs = $0.20
            ram_gb_hourly_rate: 0.05,     // 2 GB = $0.10
            network_egress_gb_rate: 0.02, // 5 GB = $0.10
        };

        let cost = snap.estimate_hourly_cost(&config, &model);
        assert!(
            (cost - 0.40).abs() < f64::EPSILON,
            "Expected 0.40, got {cost}"
        );
    }
}
