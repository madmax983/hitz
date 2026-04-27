📖 Chapter: The `hitz-api` module's domain objects (`MetricsDiff`, `SystemHealth`, `EfficiencyScore`, etc).

🔦 Insight: The structural types for our REST API were missing concrete examples. If a tired developer at 3 AM needs to know how to construct a `SystemHealth` object to test a mock endpoint, they shouldn't have to guess. I've added executable doctests to all public structs and functions that were missing them.

🧪 Example: Added executable `## Examples` doctests to `MetricsDiff`, `DiskRate`, `NetRate`, `EfficiencyScore`, `VmFingerprint`, `ImbalanceResult`, `SystemHealth`, `VmSimulator`, and `SentinelRule::evaluate`.

🖼️ Preview:
*A glorious set of copy-pasteable examples now adorns the `cargo doc` output for `hitz-api`, ensuring every struct tells a story of how it is used.*
