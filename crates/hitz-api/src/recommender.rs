//! Recommender module for suggesting scaling actions.
//!
//! # Abstract
//! This module connects the `health_check` and `classifier` modules. It takes a
//! `SystemHealth` report and a `WorkloadClass` and outputs concrete recommendations
//! for scaling the micro-VM.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{HealthStatus, SystemHealth, WorkloadClass, ScalingAction, Recommender};
//!
//! let health = SystemHealth {
//!     status: HealthStatus::Critical,
//!     reasons: vec!["Critical memory usage: 95.0%".into()],
//! };
//!
//! let class = WorkloadClass::MemoryBound;
//!
//! let actions = health.recommend(&class);
//! assert_eq!(actions, vec![ScalingAction::IncreaseRam]);
//! ```

use crate::{HealthStatus, SystemHealth, WorkloadClass};
use serde::{Deserialize, Serialize};

/// Represents an actionable scaling recommendation for the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalingAction {
    /// Suggests increasing the RAM allocated to the VM.
    IncreaseRam,
    /// Suggests increasing the number of vCPUs allocated to the VM.
    IncreaseCpu,
    /// Suggests scaling down the VM's resources as they are underutilized.
    ScaleDown,
    /// Suggests taking no scaling action.
    NoAction,
}

/// Trait for objects that can provide scaling recommendations.
pub trait Recommender {
    /// Generates a list of scaling actions based on the current workload class.
    fn recommend(&self, workload: &WorkloadClass) -> Vec<ScalingAction>;
}

impl Recommender for SystemHealth {
    fn recommend(&self, workload: &WorkloadClass) -> Vec<ScalingAction> {
        if self.status == HealthStatus::Healthy {
            if *workload == WorkloadClass::Idle {
                return vec![ScalingAction::ScaleDown];
            }
            return vec![ScalingAction::NoAction];
        }

        let mut actions = Vec::new();

        match workload {
            WorkloadClass::ComputeBound => actions.push(ScalingAction::IncreaseCpu),
            WorkloadClass::MemoryBound => actions.push(ScalingAction::IncreaseRam),
            WorkloadClass::IoHeavy => {
                // If I/O heavy but we have a health warning, usually CPU or memory is also
                // feeling the pressure. Let's suggest increasing CPU to handle I/O interrupts.
                actions.push(ScalingAction::IncreaseCpu);
            }
            WorkloadClass::Idle => {
                // If the system is mostly idle but unhealthy, this might indicate an error state,
                // but we shouldn't automatically suggest scaling up resources.
            }
        }

        if actions.is_empty() {
            actions.push(ScalingAction::NoAction);
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HealthStatus, SystemHealth, WorkloadClass};

    #[test]
    fn test_healthy_idle() {
        let health = SystemHealth {
            status: HealthStatus::Healthy,
            reasons: vec![],
        };
        let actions = health.recommend(&WorkloadClass::Idle);
        assert_eq!(actions, vec![ScalingAction::ScaleDown]);
    }

    #[test]
    fn test_healthy_compute() {
        let health = SystemHealth {
            status: HealthStatus::Healthy,
            reasons: vec![],
        };
        let actions = health.recommend(&WorkloadClass::ComputeBound);
        assert_eq!(actions, vec![ScalingAction::NoAction]);
    }

    #[test]
    fn test_unhealthy_compute() {
        let health = SystemHealth {
            status: HealthStatus::Warning,
            reasons: vec!["High CPU usage".into()],
        };
        let actions = health.recommend(&WorkloadClass::ComputeBound);
        assert_eq!(actions, vec![ScalingAction::IncreaseCpu]);
    }

    #[test]
    fn test_unhealthy_memory() {
        let health = SystemHealth {
            status: HealthStatus::Critical,
            reasons: vec!["Critical memory usage".into()],
        };
        let actions = health.recommend(&WorkloadClass::MemoryBound);
        assert_eq!(actions, vec![ScalingAction::IncreaseRam]);
    }

    #[test]
    fn test_unhealthy_io() {
        let health = SystemHealth {
            status: HealthStatus::Warning,
            reasons: vec!["Network drop".into()],
        };
        let actions = health.recommend(&WorkloadClass::IoHeavy);
        assert_eq!(actions, vec![ScalingAction::IncreaseCpu]);
    }
}
