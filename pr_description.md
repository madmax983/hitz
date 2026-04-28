💡 **The Spark:** "I noticed we collect detailed metrics and know the VM's configured size, but we don't use this combined knowledge to suggest actionable changes."
🚀 **The Feature:** "Implemented a `RightSizer` trait that analyzes `MetricsSnapshot` against `VmConfig` to emit `ResizeRecommendation` variants (scale CPU/RAM up or down)."
🔭 **The Potential:** "Could be used by the CLI to provide an `optimize` or `advise` command, or by a Kubernetes operator to autoscale micro-VMs."
⚠️ **Risk:** "Low. Isolated in `crates/hitz-api/src/rightsizer.rs` under a new `rightsizer` feature flag."
