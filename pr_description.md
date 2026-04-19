🌟 Nova: Resource Efficiency Scorer

💡 **The Spark:** We collect immense amounts of telemetry data (CPU, memory, etc.) but we don't translate that raw data into actionable insights about whether the micro-VM is actually right-sized for its workload.
🚀 **The Feature:** Implemented `EfficiencyScorer` trait and `EfficiencyScore` struct. This evaluates the `MetricsSnapshot` to determine if a VM is underutilizing its allocated CPU cores or RAM, returning a score (0-100) and human-readable optimization insights.
🔮 **The Potential:** Could be used to build auto-scaling mechanisms, provide recommendations in a dashboard, or generate rightsizing reports across fleets of VMs.
⚠️ **Risk:** Low. Additive only, isolated in `src/efficiency.rs` behind the new `efficiency` feature flag.

