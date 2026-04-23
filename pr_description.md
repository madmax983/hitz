🌟 Nova: Workload Imbalance Analyzer

💡 **The Spark:** "We have per-core CPU metrics but don't expose when a single thread is bottlenecking an otherwise idle multi-core VM."
🚀 **The Feature:** "Implemented `CoreImbalanceAnalyzer` trait to calculate per-core standard deviation and a normalized imbalance score."
🔮 **The Potential:** "Could be used to recommend downscaling vCPUs when workloads are strictly single-threaded, improving overall host density."
⚠️ **Risk:** "Low. Isolated in `crates/hitz-api/src/imbalance.rs` behind the `imbalance` feature flag."
