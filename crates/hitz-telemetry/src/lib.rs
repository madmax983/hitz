//! Telemetry and Analytics module.

#[cfg(feature = "health_check")]
/// Health assessment module for evaluating system telemetry.
pub mod health;

#[cfg(feature = "health_check")]
pub use health::{HealthCheck, HealthStatus, SystemHealth};

#[cfg(feature = "simulator")]
/// Simulator module for generating synthetic telemetry streams.
pub mod simulator;
#[cfg(feature = "simulator")]
pub use simulator::{VmSimulator, WorkloadProfile};

#[cfg(feature = "diff")]
/// Diff module for calculating rates of change between telemetry snapshots.
pub mod diff;
#[cfg(feature = "diff")]
pub use diff::{CalculateDiff, DiskRate, MetricsDiff, NetRate};

#[cfg(feature = "classifier")]
/// Classifier module for determining workload type.
pub mod classifier;
#[cfg(feature = "classifier")]
pub use classifier::{WorkloadClass, WorkloadClassifier};

#[cfg(feature = "prometheus")]
/// Prometheus module for converting metrics to Prometheus text format.
pub mod prometheus;
#[cfg(feature = "prometheus")]
pub use prometheus::ToPrometheus;

#[cfg(feature = "carbon")]
/// Carbon footprint estimation module.
pub mod carbon;
#[cfg(feature = "carbon")]
pub use carbon::{CarbonEstimator, EmissionFactors};

#[cfg(feature = "sentinel")]
/// Sentinel module for defining rules based on metrics.
pub mod sentinel;
#[cfg(feature = "sentinel")]
pub use sentinel::{ConditionOperator, MetricTarget, SentinelCondition, SentinelRule};

#[cfg(feature = "efficiency")]
/// Efficiency scoring module for evaluating resource usage.
pub mod efficiency;
#[cfg(feature = "efficiency")]
pub use efficiency::{EfficiencyScore, EfficiencyScorer};

#[cfg(feature = "fingerprint")]
/// Fingerprinting module for categorizing VM workload behavior.
pub mod fingerprint;
#[cfg(feature = "fingerprint")]
pub use fingerprint::{FingerprintGenerator, VmFingerprint};

#[cfg(feature = "imbalance")]
/// Imbalance scoring module for evaluating per-core CPU utilization imbalance.
pub mod imbalance;
#[cfg(feature = "imbalance")]
pub use imbalance::{CoreImbalanceAnalyzer, ImbalanceResult};

#[cfg(feature = "rightsizer")]
/// Rightsizing module for analyzing metrics and suggesting config changes.
pub mod rightsizer;
#[cfg(feature = "rightsizer")]
pub use rightsizer::{ResizeRecommendation, RightSizer};

#[cfg(feature = "terraform")]
/// Terraform HCL generation module.
pub mod terraform;
#[cfg(feature = "terraform")]
pub use terraform::ToTerraform;
