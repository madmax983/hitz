//! Eco Sizing impact module.
//!
//! # Abstract
//! This module projects the carbon impact of taking a resizing action.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{EcoSizer, EcoImpact, EmissionFactors, ResizeRecommendation};
//!
//! let factors = EmissionFactors::new(400.0);
//! let rec = ResizeRecommendation::ScaleDownCpu {
//!     current: 4,
//!     suggested: 2,
//!     reason: "Low usage".to_string(),
//! };
//!
//! let impact = rec.calculate_eco_impact(&factors);
//! assert!(impact.projected_emissions_change_mg_per_sec < 0.0);
//! ```

use crate::{EmissionFactors, ResizeRecommendation};
use serde::{Deserialize, Serialize};

/// Impact on carbon emissions of a proposed resize.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoImpact {
    /// The projected change in emissions in mg of CO2 per second.
    pub projected_emissions_change_mg_per_sec: f64,
}

/// Trait to project carbon impact of resizing.
pub trait EcoSizer {
    /// Calculate the projected eco impact.
    fn calculate_eco_impact(&self, factors: &EmissionFactors) -> EcoImpact;
}

impl EcoSizer for ResizeRecommendation {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_eco_impact(&self, factors: &EmissionFactors) -> EcoImpact {
        let power_delta_watts = match self {
            Self::ScaleUpCpu {
                current, suggested, ..
            } => {
                let diff_cores = f64::from(suggested.saturating_sub(*current));
                diff_cores * factors.cpu_power_watts_max
            }
            Self::ScaleDownCpu {
                current, suggested, ..
            } => {
                let diff_cores = f64::from(current.saturating_sub(*suggested));
                -(diff_cores * factors.cpu_power_watts_max)
            }
            Self::ScaleUpRam {
                current_mib,
                suggested_mib,
                ..
            } => {
                let diff_mib = f64::from(suggested_mib.saturating_sub(*current_mib));
                let diff_gb = diff_mib / 1024.0;
                diff_gb * factors.ram_power_watts_per_gb
            }
            Self::ScaleDownRam {
                current_mib,
                suggested_mib,
                ..
            } => {
                let diff_mib = f64::from(current_mib.saturating_sub(*suggested_mib));
                let diff_gb = diff_mib / 1024.0;
                -(diff_gb * factors.ram_power_watts_per_gb)
            }
        };

        let total_power_delta_watts = power_delta_watts * factors.pue;
        let power_delta_kw = total_power_delta_watts / 1000.0;
        let emissions_delta_g_per_hour = power_delta_kw * factors.grid_intensity_g_per_kwh;
        let projected_emissions_change_mg_per_sec = (emissions_delta_g_per_hour * 1000.0) / 3600.0;

        EcoImpact {
            projected_emissions_change_mg_per_sec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_scale_up() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleUpCpu {
            current: 2,
            suggested: 4,
            reason: "High usage".to_string(),
        };
        let impact = rec.calculate_eco_impact(&factors);
        // Scaling up from 2 to 4 cores should increase emissions (positive change)
        assert!(impact.projected_emissions_change_mg_per_sec > 0.0);
    }

    #[test]
    fn test_cpu_scale_down() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleDownCpu {
            current: 4,
            suggested: 2,
            reason: "Low usage".to_string(),
        };
        let impact = rec.calculate_eco_impact(&factors);
        // Scaling down from 4 to 2 cores should decrease emissions (negative change)
        assert!(impact.projected_emissions_change_mg_per_sec < 0.0);
    }

    #[test]
    fn test_ram_scale_up() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleUpRam {
            current_mib: 1024,
            suggested_mib: 2048,
            reason: "High usage".to_string(),
        };
        let impact = rec.calculate_eco_impact(&factors);
        assert!(impact.projected_emissions_change_mg_per_sec > 0.0);
    }

    #[test]
    fn test_ram_scale_down() {
        let factors = EmissionFactors::new(400.0);
        let rec = ResizeRecommendation::ScaleDownRam {
            current_mib: 2048,
            suggested_mib: 1024,
            reason: "Low usage".to_string(),
        };
        let impact = rec.calculate_eco_impact(&factors);
        assert!(impact.projected_emissions_change_mg_per_sec < 0.0);
    }
}
