1. **Create `crates/hitz-api/src/telemetry/mod.rs`**
   - Extract the telemetry analysis traits and types from multiple modules into a centralized location: `diff.rs`, `classifier.rs`, `carbon.rs`, `sentinel.rs`, `efficiency.rs`, `fingerprint.rs`, `imbalance.rs`, `rightsizer.rs`, and `prometheus.rs`.
   - Update `crates/hitz-api/src/lib.rs` to expose the new `telemetry` module and its contents.
2. **Move files into the new `telemetry` directory**
   - Move all the aforementioned files to `crates/hitz-api/src/telemetry/`.
3. **Verify compilation**
   - Run `cargo check --all-targets --all-features --workspace --exclude hitz-whp --exclude hitz-daemon --exclude hitz-vmm --exclude hitz-cli --exclude hitz-net` to ensure everything builds correctly.
4. **Complete pre commit steps to ensure proper testing, verification, review, and reflection are done.**
5. **Submit PR**
   - Use `submit` with title "🗺️ Atlas: [architectural change]"
