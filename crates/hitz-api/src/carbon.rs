//! Carbon footprint estimator for micro-VMs.
//!
//! # Abstract
//! This module provides a `CarbonEstimator` trait that connects the absolute resource
//! utilization of a system (`MetricsSnapshot`) with configurable emission factors
//! to estimate the real-time CO2 emissions of the VM.
//!
//! ## Examples
//!
//! ```rust
//! use hitz_api::{CarbonEstimator, EmissionFactors, MetricsSnapshot};
//!
//! // Assume we have a snapshot showing 50% CPU usage
//! // let snap: MetricsSnapshot = ...;
//! //
//! // // Using a data center in a region with 400 gCO2eq/kWh
//! // let factors = EmissionFactors::new(400.0);
//! // let emissions = snap.estimate_carbon(&factors, 4); // 4 CPUs
//! // println!("Current emission rate: {:.2} mg CO2/sec", emissions);
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Configurable emission factors for the data center region.
///
/// # Abstract
/// This struct holds the assumptions needed to translate power consumption
/// in Watts into carbon emissions in grams of CO2 equivalent (gCO2eq).
///
/// # Details
/// It includes region-specific grid intensity alongside assumed hardware
/// efficiency constants like CPU max/idle power and data center PUE.
///
/// ## Examples
///
/// ```rust
/// use hitz_api::EmissionFactors;
///
/// // Create a profile for a relatively dirty grid (400 g/kWh)
/// let profile = EmissionFactors::new(400.0);
/// assert_eq!(profile.pue, 1.5);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmissionFactors {
    /// Grid carbon intensity in grams of CO2 equivalent per kilowatt-hour (gCO2eq/kWh).
    pub grid_intensity_g_per_kwh: f64,
    /// Assumed power consumption per CPU core at 100% utilization in Watts.
    pub cpu_power_watts_max: f64,
    /// Assumed power consumption per CPU core at 0% utilization (idle) in Watts.
    pub cpu_power_watts_idle: f64,
    /// Assumed power consumption per GB of RAM in Watts.
    pub ram_power_watts_per_gb: f64,
    /// Power Usage Effectiveness (PUE) of the data center.
    pub pue: f64,
}

impl EmissionFactors {
    /// Creates a new `EmissionFactors` profile with reasonable defaults for
    /// a given grid intensity.
    ///
    /// # Abstract
    /// A quick factory method to instantiate emission factors without manually
    /// defining power constants for RAM, CPU idle, or PUE.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::EmissionFactors;
    ///
    /// // Standard global defaults with a specific grid intensity
    /// let factors = EmissionFactors::new(350.0);
    /// assert_eq!(factors.cpu_power_watts_max, 10.0);
    /// assert_eq!(factors.ram_power_watts_per_gb, 0.5);
    /// ```
    #[must_use]
    pub const fn new(grid_intensity_g_per_kwh: f64) -> Self {
        Self {
            grid_intensity_g_per_kwh,
            // Typical server x86 core estimates
            cpu_power_watts_max: 10.0,
            cpu_power_watts_idle: 2.0,
            // Typical RAM power estimates
            ram_power_watts_per_gb: 0.5,
            // Global average PUE is ~1.5
            pue: 1.5,
        }
    }
}

/// Trait for objects that can estimate their carbon footprint.
///
/// # Abstract
/// This trait defines the capability to estimate the real-time CO2 emissions
/// of a micro-VM based on its current resource utilization.
pub trait CarbonEstimator {
    /// Estimates the current rate of carbon emissions in milligrams of CO2 per second (mg CO2/sec).
    ///
    /// # Abstract
    /// Applies a power consumption model to a utilization snapshot to estimate
    /// the instantaneous carbon emission rate.
    ///
    /// # Details
    /// Requires the number of allocated virtual CPUs (`vcpus`) because `MetricsSnapshot`
    /// stores CPU percentage but does not intrinsically know the total core count if some
    /// cores were completely idle (0%).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_api::{CarbonEstimator, EmissionFactors, MetricsSnapshot, CpuMetrics, MemoryMetrics};
    ///
    /// let snap = MetricsSnapshot {
    ///     timestamp_ms: 1000,
    ///     cpu: CpuMetrics {
    ///         total_pct: 100.0,
    ///         per_core: vec![100.0, 100.0],
    ///         load_avg: [1.0, 1.0, 1.0],
    ///     },
    ///     memory: MemoryMetrics {
    ///         total_bytes: 1024 * 1024 * 1024, // 1 GB
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
    /// let factors = EmissionFactors::new(200.0);
    /// let emissions = snap.estimate_carbon(&factors, 2);
    /// assert!(emissions > 0.0);
    /// ```
    fn estimate_carbon(&self, factors: &EmissionFactors, vcpus: u32) -> f64;
}

impl CarbonEstimator for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn estimate_carbon(&self, factors: &EmissionFactors, vcpus: u32) -> f64 {
        // CPU Power Model
        // Interpolate between idle and max based on total percentage.
        // total_pct is 0.0 to 100.0.
        let cpu_utilization = f64::from(self.cpu.total_pct).clamp(0.0, 100.0) / 100.0;
        let vcpus_f64 = f64::from(vcpus);

        let cpu_power_watts = vcpus_f64.mul_add(
            factors.cpu_power_watts_idle,
            (factors.cpu_power_watts_max - factors.cpu_power_watts_idle)
                * vcpus_f64
                * cpu_utilization,
        );

        // RAM Power Model
        let ram_gb = self.memory.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_power_watts = ram_gb * factors.ram_power_watts_per_gb;

        // Total power including data center overhead
        let total_power_watts = (cpu_power_watts + ram_power_watts) * factors.pue;

        // Convert power to emissions
        // Power in kW
        let power_kw = total_power_watts / 1000.0;

        // Emissions per hour in grams (gCO2eq/hour)
        let emissions_g_per_hour = power_kw * factors.grid_intensity_g_per_kwh;

        // Convert to milligrams per second (mgCO2eq/sec)
        // 1 gram = 1000 mg
        // 1 hour = 3600 seconds
        (emissions_g_per_hour * 1000.0) / 3600.0
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    fn base_snapshot(cpu_pct: f32, ram_bytes: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: cpu_pct,
                per_core: vec![cpu_pct],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: ram_bytes,
                used_bytes: ram_bytes / 2,
                free_bytes: ram_bytes / 2,
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
    fn test_carbon_estimator_clamped_utilization() {
        let factors = EmissionFactors::new(400.0);
        let snap_high = base_snapshot(150.0, 2 * 1024 * 1024 * 1024);
        let snap_low = base_snapshot(-10.0, 2 * 1024 * 1024 * 1024);
        let snap_max = base_snapshot(100.0, 2 * 1024 * 1024 * 1024);
        let snap_idle = base_snapshot(0.0, 2 * 1024 * 1024 * 1024);

        let emissions_high = snap_high.estimate_carbon(&factors, 4);
        let emissions_low = snap_low.estimate_carbon(&factors, 4);
        let emissions_max = snap_max.estimate_carbon(&factors, 4);
        let emissions_idle = snap_idle.estimate_carbon(&factors, 4);

        assert!((emissions_high - emissions_max).abs() < f64::EPSILON);
        assert!((emissions_low - emissions_idle).abs() < f64::EPSILON);
    }

    #[test]
    fn test_carbon_estimator_idle() {
        let factors = EmissionFactors::new(400.0);
        let snap = base_snapshot(0.0, 2 * 1024 * 1024 * 1024); // 0% CPU, 2GB RAM

        // Expected CPU Watts: 4 vcpus * 2.0 (idle) = 8.0 W
        // Expected RAM Watts: 2 GB * 0.5 = 1.0 W
        // Total IT Watts: 9.0 W
        // With PUE 1.5: 13.5 W = 0.0135 kW
        // Emissions (g/hr): 0.0135 * 400.0 = 5.4 g/hr
        // Emissions (mg/s): 5.4 * 1000 / 3600 = 1.5 mg/s

        let emissions = snap.estimate_carbon(&factors, 4);
        assert!(
            (emissions - 1.5).abs() < f64::EPSILON,
            "Expected 1.5 mg/s, got {emissions}"
        );
    }

    #[test]
    fn test_carbon_estimator_full_load() {
        let factors = EmissionFactors::new(400.0);
        let snap = base_snapshot(100.0, 2 * 1024 * 1024 * 1024); // 100% CPU, 2GB RAM

        // Expected CPU Watts: 4 vcpus * 10.0 (max) = 40.0 W
        // Expected RAM Watts: 2 GB * 0.5 = 1.0 W
        // Total IT Watts: 41.0 W
        // With PUE 1.5: 61.5 W = 0.0615 kW
        // Emissions (g/hr): 0.0615 * 400.0 = 24.6 g/hr
        // Emissions (mg/s): 24.6 * 1000 / 3600 = 6.8333... mg/s

        let emissions = snap.estimate_carbon(&factors, 4);
        assert!(
            (emissions - 6.833333333333333).abs() < 1e-10,
            "Got {emissions}"
        );
    }
}
