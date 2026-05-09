💡 What: The optimization implemented
Eliminated multiple redundant `.clone()` and `.to_string()` heap allocations on the `hitz-cli` table rendering hot path. Comfy-table's `Cell::new` directly accepts `AsRef<str>` and basic primitives, allowing us to pass `&disk.name` and numeric values directly instead of unnecessarily copying strings to the heap.

🎯 Why: The bottleneck
When running continuous polling or displaying full dashboard metrics, the CLI previously invoked multiple unnecessary string allocations for every single disk, network interface, and process row. This generated unneeded garbage and wasted CPU cycles formatting static data into strings before giving them to the table.

📊 Impact: Expected gain
Reduces memory allocation overhead significantly per CLI refresh tick on busy VMs, making the CLI dashboard snappier and reducing overall memory footprint.

🔬 Measurement: How to verify
Run the `hitz-cli` binary and use `hitz top` or `hitz status` to visually confirm that the metric tables render correctly with the exact same format as before. Cross-compiled with `cargo clippy -p hitz-cli --bin hitz --target x86_64-pc-windows-msvc`. (Note: `cargo test` is skipped for `hitz-cli` natively on Linux due to MSVC linker requirements, but static compilation passes).
