🌟 Nova: Cost Estimation for Micro-VMs

💡 **The Spark:** "I noticed we gather extensive resource utilization metrics, but we have no way to translate those metrics into a financial view. We need to be able to tell users how much a micro-VM actually costs to run based on its current resource allocation and utilization."

🚀 **The Feature:** "Implemented `CostEstimator` trait in the `hitz-api` crate. It connects the absolute resource utilization of a system (`MetricsSnapshot`) with configurable pricing factors (`PricingFactors`) to estimate the real-time financial cost of running the VM. Included a `with_premium` method to dynamically adjust cost based on high CPU utilization."

🔮 **The Potential:** "Could be used for creating detailed cost reports for users, dynamic scaling algorithms to minimize expenses, or setting financial guardrails where micro-VMs are shut down if they become too expensive."

⚠️ **Risk:** "Low. Isolated in `crates/hitz-api/src/cost.rs` and placed behind a `cost` feature flag."
