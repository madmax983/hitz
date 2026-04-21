🌟 Nova: Cloud FinOps Cost Simulator (`billing` feature)

💡 The Spark
We collect highly detailed, real-time resource utilization metrics (`MetricsSnapshot`) from our micro-VMs, but we don't translate that usage into actual dollars. In modern cloud environments, understanding the financial impact of resource usage is critical for FinOps.

🚀 The Feature
Implemented a new `billing` module (behind the `billing` Cargo feature flag) that introduces a `BillingEstimator` trait for `MetricsSnapshot`. This connects absolute resource utilization with configurable `ServerlessPricingModel`s to estimate the real-time cost run-rate (in USD/hour) based on active CPU cores and actively used RAM.

🔭 The Potential
This enables the daemon or CLI to stream real-time cost estimations alongside performance metrics, allowing users to build internal chargeback systems, simulate serverless pricing models, and identify idle resources burning cash.

⚠️ Risk
Low. The logic is entirely self-contained within `crates/hitz-api/src/billing.rs`, strictly adheres to floating point precision best practices via casts, and is isolated behind a Cargo feature flag. Core daemon and VM logic remain completely untouched.
