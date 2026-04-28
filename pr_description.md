📖 Chapter: The `CalculateDiff` and `EfficiencyScorer` traits and their associated structs (`MetricsDiff`, `DiskRate`, `NetRate`, `EfficiencyScore`).

🔦 Insight: Added executable `///` doc tests (`## Examples`) to key structures and methods in `diff.rs` and `efficiency.rs`. This provides clear, copy-pasteable usage examples for developers, explaining not just *what* the structures and functions do, but *how* to use them effectively in telemetry pipelines.

🧪 Example: Added 2 executable doctests (one for `CalculateDiff::diff` and one for `EfficiencyScore`/`EfficiencyScorer::calculate_efficiency`) to demonstrate real-world telemetry evaluation.

🖼️ Preview: Evaluated successfully via `cargo doc --open`.
