#![allow(clippy::unreadable_literal)]
//! EcoScaler module for analyzing the carbon impact of VM rightsizing.
//!
//! # Abstract
//! Connects the `CarbonEstimator` constants from `EmissionFactors` with
//! `ResizeRecommendation` to calculate the exact delta in carbon emissions
//! when scaling a VM up or down.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{EcoScaler, EmissionFactors, ResizeRecommendation};
//!
//! let factors = EmissionFactors::new(400.0);
//! let rec = ResizeRecommendation::ScaleDownRam {
//!     current_mib: 4096,
//!     suggested_mib: 2048,
//!     reason: "Low RAM".into(),
//! };
//!
//! let delta = rec.estimate_carbon_delta(&factors);
//! assert!(delta < 0.0); // We save carbon!
//! ```

use crate::{EmissionFactors, ResizeRecommendation};

/// Trait to estimate the change in carbon emissions based on a resize recommendation.
pub trait EcoScaler {
    /// Estimates the change in carbon emissions in milligrams of CO2 per second (mg CO2/sec).
    /// Positive values mean an increase in emissions, negative mean a reduction.
    fn estimate_carbon_delta(&self, factors: &EmissionFactors) -> f64;
}

impl EcoScaler for ResizeRecommendation {
    #[allow(clippy::cast_precision_loss)]
    fn estimate_carbon_delta(&self, factors: &EmissionFactors) -> f64 {
        let (cpu_diff, ram_gb_diff) = match self {
            Self::ScaleUpCpu {
                current, suggested, ..
            }
            | Self::ScaleDownCpu {
                current, suggested, ..
            } => (f64::from(*suggested) - f64::from(*current), 0.0),
            Self::ScaleUpRam {
                current_mib,
                suggested_mib,
                ..
            }
            | Self::ScaleDownRam {
                current_mib,
                suggested_mib,
                ..
            } => (
                0.0,
                (f64::from(*suggested_mib) - f64::from(*current_mib)) / 1024.0,
            ),
        };

        // When rightsizing, we only care about the *idle* cost of having that capacity available.
        let cpu_power_watts = cpu_diff * factors.cpu_power_watts_idle;
        let ram_power_watts = ram_gb_diff * factors.ram_power_watts_per_gb;

        let total_power_watts = (cpu_power_watts + ram_power_watts) * factors.pue;
        let power_kw = total_power_watts / 1000.0;
        let emissions_g_per_hour = power_kw * factors.grid_intensity_g_per_kwh;

        (emissions_g_per_hour * 1000.0) / 3600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_up_cpu_impact() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleUpCpu {
            current: 2,
            suggested: 4,
            reason: "High CPU".into(),
        };
        // 2 extra vCPUs = 2 * 2.0W (idle) = 4.0W
        // 4.0W * 1.5 PUE = 6.0W = 0.006kW
        // 0.006kW * 400.0 g/kWh = 2.4 g/h
        // 2.4 g/h * 1000 / 3600 = 0.6666... mg/s
        let delta = rec.estimate_carbon_delta(&factors);
        assert!((delta - 0.6666666666666666).abs() < 1e-10, "Got {delta}");
    }

    #[test]
    fn test_scale_down_ram_impact() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleDownRam {
            current_mib: 4096,
            suggested_mib: 2048,
            reason: "Low RAM".into(),
        };
        // 2048 - 4096 = -2048 MiB = -2.0 GiB
        // -2.0 GiB * 0.5W/GB = -1.0W
        // -1.0W * 1.5 PUE = -1.5W = -0.0015kW
        // -0.0015kW * 400.0 g/kWh = -0.6 g/h
        // -0.6 g/h * 1000 / 3600 = -0.1666... mg/s
        let delta = rec.estimate_carbon_delta(&factors);
        assert!((delta - -0.1666666666666666).abs() < 1e-10, "Got {delta}");
    }
}
