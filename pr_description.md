🎯 Target: `MetricsRequest` serialization in `hitz-api::metrics`.
💣 Risk: If the JSON representation of `MetricsRequest::Snapshot` changes due to an unintended modification, it could break compatibility between the CLI and the daemon over the REST API.
🧪 Strategy: Added a new unit test `should_serialize_metrics_request_to_exact_json` in `crates/hitz-api/src/metrics_test.rs` to assert the exact JSON wire format (`"\"Snapshot\""`) directly.
🔬 Verification: Run `cargo test -p hitz-api --all-features` to ensure the new serialization test passes.
