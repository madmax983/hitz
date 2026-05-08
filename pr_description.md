💡 What: Replaced a `format!` macro that was used to initialize the `hcl` string with `String::with_capacity` and an initial `writeln!`. Also pre-allocate capacity for `handles` vec in `fuzz_serial_buf.rs`.
🎯 Why: In `to_terraform`, creating an initial string using `format!` involves an unnecessary heap allocation right before a series of `writeln!` appends. Same goes for vectors growing without initial capacity.
📊 Impact: Eliminates an intermediate string allocation, saving one heap allocation when generating the Terraform configuration. Optimizes fuzzer as well.
🔬 Measurement: Run `cargo test -p hitz-api` to ensure no regressions in Terraform output format.
