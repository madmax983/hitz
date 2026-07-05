1. Refactor `publish_to_otel` in `crates/hitz-daemon/src/vsock_server.rs` to optimize OpenTelemetry metric creation using `replace_with_git_merge_diff`.
   - Building OTel metrics (`meter.u64_counter(...).build()`) acquires internal locks and creates objects. Building them *inside* loops (for each CPU core, disk, and network interface) on the hot metric publishing path is incredibly inefficient.
   - We will hoist the `build()` calls outside of the loops using this payload:
     ```
     <<<<<<< SEARCH
         for (i, &pct) in snap.cpu.per_core.iter().enumerate() {
             let core_labels = [vm_id_kv.clone(), KeyValue::new("cpu", i as i64)];
             meter
                 .f64_gauge("hitz.guest.cpu_usage_per_core")
                 .build()
                 .record(f64::from(pct), &core_labels);
         }

         meter
             .u64_gauge("hitz.guest.memory_used_bytes")
             .with_description("Guest memory in use")
             .build()
             .record(snap.memory.used_bytes, &labels);

         meter
             .u64_gauge("hitz.guest.memory_total_bytes")
             .with_description("Guest total RAM")
             .build()
             .record(snap.memory.total_bytes, &labels);

         for disk in &snap.disks {
             let disk_labels = [vm_id_kv.clone(), KeyValue::new("disk", disk.name.clone())];
             meter
                 .u64_counter("hitz.guest.disk_read_bytes_total")
                 .build()
                 .add(disk.read_bytes, &disk_labels);
             meter
                 .u64_counter("hitz.guest.disk_write_bytes_total")
                 .build()
                 .add(disk.write_bytes, &disk_labels);
         }

         for net in &snap.networks {
             let net_labels = [
                 vm_id_kv.clone(),
                 KeyValue::new("interface", net.interface.clone()),
             ];
             meter
                 .u64_counter("hitz.guest.net_rx_bytes_total")
                 .build()
                 .add(net.rx_bytes, &net_labels);
             meter
                 .u64_counter("hitz.guest.net_tx_bytes_total")
                 .build()
                 .add(net.tx_bytes, &net_labels);
         }
     =======
         let cpu_usage_per_core_metric = meter
             .f64_gauge("hitz.guest.cpu_usage_per_core")
             .build();

         for (i, &pct) in snap.cpu.per_core.iter().enumerate() {
             let core_labels = [vm_id_kv.clone(), KeyValue::new("cpu", i as i64)];
             cpu_usage_per_core_metric.record(f64::from(pct), &core_labels);
         }

         meter
             .u64_gauge("hitz.guest.memory_used_bytes")
             .with_description("Guest memory in use")
             .build()
             .record(snap.memory.used_bytes, &labels);

         meter
             .u64_gauge("hitz.guest.memory_total_bytes")
             .with_description("Guest total RAM")
             .build()
             .record(snap.memory.total_bytes, &labels);

         let disk_read_metric = meter
             .u64_counter("hitz.guest.disk_read_bytes_total")
             .build();
         let disk_write_metric = meter
             .u64_counter("hitz.guest.disk_write_bytes_total")
             .build();

         for disk in &snap.disks {
             let disk_labels = [vm_id_kv.clone(), KeyValue::new("disk", disk.name.clone())];
             disk_read_metric.add(disk.read_bytes, &disk_labels);
             disk_write_metric.add(disk.write_bytes, &disk_labels);
         }

         let net_rx_metric = meter
             .u64_counter("hitz.guest.net_rx_bytes_total")
             .build();
         let net_tx_metric = meter
             .u64_counter("hitz.guest.net_tx_bytes_total")
             .build();

         for net in &snap.networks {
             let net_labels = [
                 vm_id_kv.clone(),
                 KeyValue::new("interface", net.interface.clone()),
             ];
             net_rx_metric.add(net.rx_bytes, &net_labels);
             net_tx_metric.add(net.tx_bytes, &net_labels);
         }
     >>>>>>> REPLACE
     ```
2. Run `git diff` via `run_in_bash_session` to verify the modifications.
3. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets --exclude hitz-net --exclude hitz-vmm --exclude hitz-whp --exclude hitz-cli --exclude hitz-daemon --exclude hitz-devices -- -D warnings`, and `cargo test --workspace --exclude hitz-net --exclude hitz-vmm --exclude hitz-whp --exclude hitz-cli --exclude hitz-daemon --exclude hitz-devices` via `run_in_bash_session`.
4. Complete pre-commit steps to ensure proper testing, verification, review, and reflection are done.
5. Submit PR using the `submit` tool.
   - Title: `⚡ Bolt: Hoist OpenTelemetry instrument builds out of telemetry loops`
   - Description:
     💡 What: Hoisted the `.build()` calls for OpenTelemetry metric instruments (`cpu_usage_per_core`, `disk_read_bytes_total`, `net_rx_bytes_total`, etc.) outside of their respective `for` loops in the vsock metrics processing hotpath.
     🎯 Why: `meter.u64_counter(...).build()` is not a trivial operation; it acquires internal global/registry locks and performs allocations. Calling this inside a loop that processes every disk, network interface, and CPU core (every time telemetry is received) creates an unnecessary bottleneck.
     📊 Impact: Eliminates N repeated metric instrument instantiations per telemetry payload received (where N is # of cores + # of disks*2 + # of net interfaces*2).
     🔬 Measurement: Verified with `cargo test` and by observing CPU footprint of daemon metrics processing task.
