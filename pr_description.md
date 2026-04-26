🌟 Nova: Rightsizing Recommendation Engine

💡 The Spark: I noticed we collect rich telemetry metrics (CPU usage per core, memory usage, swap), but we leave the burden of interpreting these metrics and scaling the VM entirely on the user. We needed a way to translate raw data into actionable scaling advice.

🚀 The Feature: Implemented the `RecommendationEngine` trait and `VmRecommendation` types in a new `recommendation` module behind a cargo feature flag. It analyzes real-time `MetricsSnapshot` data to recommend upsizing or downsizing CPU and memory based on configurable utilization thresholds and swap activity.

🔭 The Potential: This lays the foundation for an "Auto-Scaler" daemon that could automatically resize VMs dynamically based on workload demands, optimizing cloud costs and preventing OOM/CPU starvation.

⚠️ Risk: Low. It's completely isolated in `crates/hitz-api/src/recommendation.rs`, hidden behind the `recommendation` feature flag, and only analyzes existing data (no state mutation or runtime overhead).
