//! Carbon forecast module.
//!
//! # Abstract
//! This module estimates the future CO2 emissions of a virtual machine
//! given a simulated workload profile and data center emission factors.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::carbon_forecast::CarbonForecaster;
//! use hitz_api::simulator::WorkloadProfile;
//! use hitz_api::EmissionFactors;
//!
//! let factors = EmissionFactors::new(400.0);
//! let forecaster = CarbonForecaster::new(factors, 4);
//! let total_emissions_mg = forecaster.forecast(WorkloadProfile::CpuSpike, 60);
//! assert!(total_emissions_mg > 0.0);
//! ```

use crate::carbon::{CarbonEstimator, EmissionFactors};
use crate::simulator::{VmSimulator, WorkloadProfile};

/// Forecasts carbon emissions over time based on a simulated workload.
#[derive(Debug)]
pub struct CarbonForecaster {
    factors: EmissionFactors,
    vcpus: u32,
}

impl CarbonForecaster {
    /// Creates a new carbon forecaster.
    #[must_use]
    pub const fn new(factors: EmissionFactors, vcpus: u32) -> Self {
        Self { factors, vcpus }
    }

    /// Forecasts the total CO2 emissions (in mg) over a given duration (in seconds).
    #[must_use]
    pub fn forecast(&self, profile: WorkloadProfile, duration_secs: usize) -> f64 {
        let simulator = VmSimulator::new(profile);
        // Simulator returns one snapshot per second.
        simulator
            .take(duration_secs)
            .map(|snap| snap.estimate_carbon(&self.factors, self.vcpus))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carbon_forecast_idle() {
        let factors = EmissionFactors::new(400.0);
        let forecaster = CarbonForecaster::new(factors, 4);
        let total_emissions = forecaster.forecast(WorkloadProfile::Idle, 10);
        assert!(total_emissions > 0.0);
    }
}
