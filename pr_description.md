🛡️ Sentry: [test coverage improvement]

🎯 Target: `hitz-api::sentinel::SentinelRule::evaluate`
💣 Risk: The `match` arms evaluating conditional operators (`GreaterThan`, `LessThan`, `Equals`) against targets (`CpuTotalPct`, `MemoryUsedBytes`) lacked comprehensive testing to ensure accurate alerting behavior and edge-case resilience.
🧪 Strategy: Introduced a table-driven unit test `should_evaluate_all_conditions_correctly` in the `tests` module to rigorously iterate through permutations of targets, operators, precision edge cases (epsilon tolerance), and boundary metrics.
🔭 Verification: `cargo test --workspace --exclude hitz-whp --exclude hitz-cli --exclude hitz-daemon --exclude hitz-net --exclude hitz-devices --exclude hitz-vmm --exclude hitz-guest-agent --all-features`
