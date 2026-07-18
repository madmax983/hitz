//! EcoSizer: The Mashup of Rightsizing and Carbon Estimation.
//!
//! # Abstract
//! Calculates the potential environmental impact (carbon savings or cost)
//! of a rightsizing recommendation.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ResizeRecommendation, EmissionFactors, EcoImpact};
//!
//! let rec = ResizeRecommendation::ScaleDownCpu {
//!     current: 4,
//!     suggested: 2,
//!     reason: "Low utilization".to_string(),
//! };
//!
//! let factors = EmissionFactors::new(400.0);
//! let impact = rec.estimate_carbon_impact(&factors);
//! assert!(impact < 0.0); // Negative means we are saving carbon!
//! ```

use crate::{EmissionFactors, ResizeRecommendation};

/// Trait to estimate the environmental impact of a resize recommendation.
pub trait EcoImpact {
    /// Returns the estimated change in carbon emissions (mg CO2/sec).
    /// A negative value represents carbon savings, while a positive value represents a carbon cost.
    fn estimate_carbon_impact(&self, factors: &EmissionFactors) -> f64;
}

impl EcoImpact for ResizeRecommendation {
    fn estimate_carbon_impact(&self, factors: &EmissionFactors) -> f64 {
        let mut cpu_diff: f64 = 0.0;
        let mut ram_diff_gb: f64 = 0.0;

        match self {
            Self::ScaleUpCpu {
                current, suggested, ..
            }
            | Self::ScaleDownCpu {
                current, suggested, ..
            } => {
                cpu_diff = f64::from(*suggested) - f64::from(*current);
            }
            Self::ScaleUpRam {
                current_mib,
                suggested_mib,
                ..
            }
            | Self::ScaleDownRam {
                current_mib,
                suggested_mib,
                ..
            } => {
                ram_diff_gb = (f64::from(*suggested_mib) - f64::from(*current_mib)) / 1024.0;
            }
        }

        // Calculate power difference in Watts
        // We assume the change in CPU allocation affects the idle power baseline and max potential.
        // For simplicity of the forecast, we use the max power to show max potential impact.
        let cpu_power_diff = cpu_diff * factors.cpu_power_watts_max;
        let ram_power_diff = ram_diff_gb * factors.ram_power_watts_per_gb;

        let total_power_diff_watts = (cpu_power_diff + ram_power_diff) * factors.pue;

        let power_kw = total_power_diff_watts / 1000.0;
        let emissions_g_per_hour = power_kw * factors.grid_intensity_g_per_kwh;

        (emissions_g_per_hour * 1000.0) / 3600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_down_cpu_impact() {
        let rec = ResizeRecommendation::ScaleDownCpu {
            current: 4,
            suggested: 2,
            reason: String::new(),
        };
        let factors = EmissionFactors::new(400.0);
        let impact = rec.estimate_carbon_impact(&factors);

        // current: 4, suggested: 2 -> diff: -2 cpus
        // cpu power diff: -2 * 10.0 = -20W
        // total power diff (PUE 1.5): -30W = -0.03kW
        // emissions_g_per_hour = -0.03 * 400.0 = -12.0 g/hr
        // mg/sec = -12.0 * 1000.0 / 3600.0 = -3.333... mg/sec
        assert!(impact < 0.0);
        assert!((impact - (-3.3333333333333335)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scale_up_ram_impact() {
        let rec = ResizeRecommendation::ScaleUpRam {
            current_mib: 1024,
            suggested_mib: 2048,
            reason: String::new(),
        };
        let factors = EmissionFactors::new(400.0);
        let impact = rec.estimate_carbon_impact(&factors);

        // current: 1024, suggested: 2048 -> diff: 1 GB
        // ram power diff: 1 * 0.5 = 0.5W
        // total power diff (PUE 1.5): 0.75W = 0.00075kW
        // emissions_g_per_hour = 0.00075 * 400.0 = 0.3 g/hr
        // mg/sec = 0.3 * 1000.0 / 3600.0 = 0.08333... mg/sec
        assert!(impact > 0.0);
        assert!((impact - 0.08333333333333333).abs() < f64::EPSILON);
    }
}
