use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Eco footprint estimation for a system snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcoFootprint {
    /// Estimated power consumption in Watts.
    pub power_watts: f64,
    /// Estimated Carbon Intensity (gCO2eq/kWh).
    /// Hardcoded global average or configurable in the future.
    pub carbon_intensity: f64,
    /// Estimated Carbon emissions (gCO2eq/hr).
    pub emissions_gco2_per_hr: f64,
}

/// Trait to calculate environmental impact from a metrics snapshot.
pub trait EcoMetrics {
    /// Calculate the estimated eco footprint based on resource utilisation.
    fn calculate_footprint(&self) -> EcoFootprint;
}

impl EcoMetrics for MetricsSnapshot {
    fn calculate_footprint(&self) -> EcoFootprint {
        // Base idle power consumption for a microVM is roughly 2.0 Watts
        let base_power = 2.0_f64;

        // Power increases linearly with CPU load. Assuming 100% CPU on 1 core = 5 Watts
        let cores = if self.cpu.per_core.is_empty() { 1.0_f64 } else { self.cpu.per_core.len() as f64 };
        // Max theoretical power for CPU is 5 Watts per core
        let max_cpu_power = cores * 5.0_f64;

        // We calculate cpu utilization as a percentage of total capacity (total_pct can be over 100 if multiple cores)
        // Actually total_pct is usually average percentage 0-100 across all cores.
        let cpu_power = (f64::from(self.cpu.total_pct) / 100.0_f64) * max_cpu_power;

        // Memory power consumption. Let's assume 1 GiB used = 1 Watt
        let mem_power = (self.memory.used_bytes as f64) / (1024.0 * 1024.0 * 1024.0) * 1.0_f64;

        let total_power_watts = base_power + cpu_power + mem_power;

        // Global average carbon intensity is roughly 436 gCO2eq/kWh
        let carbon_intensity = 436.0_f64;

        // kW = Watts / 1000
        let kw = total_power_watts / 1000.0_f64;
        let emissions = kw * carbon_intensity;

        EcoFootprint {
            power_watts: total_power_watts,
            carbon_intensity,
            emissions_gco2_per_hr: emissions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics, NetMetrics};

    fn dummy_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0, 50.0],
                load_avg: [0.5, 0.5, 0.5],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
                free_bytes: 512 * 1024 * 1024,
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
    fn test_calculate_footprint() {
        let snap = dummy_snapshot();
        let footprint = snap.calculate_footprint();

        // Base: 2.0W
        // CPU: 2 cores * 5W * 0.5 = 5.0W
        // Memory: 512 MiB = 0.5 GiB = 0.5W
        // Total = 7.5W
        assert_eq!(footprint.power_watts, 7.5);
        assert_eq!(footprint.carbon_intensity, 436.0);
        // Emissions = 7.5 / 1000 * 436.0 = 3.27
        assert_eq!(footprint.emissions_gco2_per_hr, 3.27);
    }
}
