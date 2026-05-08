//! Green scaler module for projecting carbon impact of scaling actions.
//!
//! # Abstract
//! This module provides a `GreenScaler` trait to quantify the environmental impact
//! (in CO2 emissions) of applying a rightsizing recommendation.

use crate::{EmissionFactors, ResizeRecommendation};
use serde::{Deserialize, Serialize};

/// The projected carbon impact of a scaling action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarbonImpact {
    /// The change in carbon emissions in mg CO2 per second. Positive means more emissions, negative means savings.
    pub delta_mg_co2_per_sec: f64,
}

/// Trait to project the carbon impact of a scaling action.
pub trait GreenScaler {
    /// Projects the carbon impact given the current emission factors.
    fn project_carbon_impact(&self, factors: &EmissionFactors) -> CarbonImpact;
}

impl GreenScaler for ResizeRecommendation {
    #[allow(clippy::suboptimal_flops)]
    fn project_carbon_impact(&self, factors: &EmissionFactors) -> CarbonImpact {
        let delta_watts = match self {
            Self::ScaleUpCpu {
                current, suggested, ..
            }
            | Self::ScaleDownCpu {
                current, suggested, ..
            } => {
                f64::from(*suggested) * factors.cpu_power_watts_idle
                    - f64::from(*current) * factors.cpu_power_watts_idle
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
                let current_gb = f64::from(*current_mib) / 1024.0;
                let suggested_gb = f64::from(*suggested_mib) / 1024.0;
                (suggested_gb - current_gb) * factors.ram_power_watts_per_gb
            }
        };

        let power_kw = (delta_watts * factors.pue) / 1000.0;
        let emissions_g_per_hour = power_kw * factors.grid_intensity_g_per_kwh;
        let delta_mg_co2_per_sec = (emissions_g_per_hour * 1000.0) / 3600.0;

        CarbonImpact {
            delta_mg_co2_per_sec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_down_cpu() {
        let rec = ResizeRecommendation::ScaleDownCpu {
            current: 4,
            suggested: 2,
            reason: "".into(),
        };
        let factors = EmissionFactors::new(400.0);
        let impact = rec.project_carbon_impact(&factors);
        assert!(impact.delta_mg_co2_per_sec < 0.0);
    }

    #[test]
    fn test_scale_up_ram() {
        let rec = ResizeRecommendation::ScaleUpRam {
            current_mib: 1024,
            suggested_mib: 2048,
            reason: "".into(),
        };
        let factors = EmissionFactors::new(400.0);
        let impact = rec.project_carbon_impact(&factors);
        assert!(impact.delta_mg_co2_per_sec > 0.0);
    }
}
