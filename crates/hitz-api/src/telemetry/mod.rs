#[cfg(feature = "carbon")]
pub mod carbon;
#[cfg(feature = "classifier")]
pub mod classifier;
#[cfg(feature = "diff")]
pub mod diff;
#[cfg(feature = "efficiency")]
pub mod efficiency;
#[cfg(feature = "fingerprint")]
pub mod fingerprint;
#[cfg(feature = "imbalance")]
pub mod imbalance;
#[cfg(feature = "prometheus")]
pub mod prometheus;
#[cfg(feature = "rightsizer")]
pub mod rightsizer;
#[cfg(feature = "sentinel")]
pub mod sentinel;

#[cfg(feature = "carbon")]
pub use carbon::{CarbonEstimator, EmissionFactors};
#[cfg(feature = "classifier")]
pub use classifier::{WorkloadClass, WorkloadClassifier};
#[cfg(feature = "diff")]
pub use diff::{CalculateDiff, DiskRate, MetricsDiff, NetRate};
#[cfg(feature = "efficiency")]
pub use efficiency::{EfficiencyScore, EfficiencyScorer};
#[cfg(feature = "fingerprint")]
pub use fingerprint::{FingerprintGenerator, VmFingerprint};
#[cfg(feature = "imbalance")]
pub use imbalance::{CoreImbalanceAnalyzer, ImbalanceResult};
#[cfg(feature = "prometheus")]
pub use prometheus::ToPrometheus;
#[cfg(feature = "rightsizer")]
pub use rightsizer::{ResizeRecommendation, RightSizer};
#[cfg(feature = "sentinel")]
pub use sentinel::{ConditionOperator, MetricTarget, SentinelCondition, SentinelRule};
