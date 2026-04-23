1. **Target**: `handle_vm_list` in `crates/hitz-cli/src/main.rs`.
2. **Issue**: When `handle_vm_list` receives a list of VMs from the daemon, it correctly creates a `comfy_table::Table`, but does not apply `comfy_table::Attribute::Bold` to the header row. This violates the `Visual Hierarchy: Important information must pop` boundary. Other functions like `format_metrics_snapshot`, `handle_vm_status`, and `handle_vm_analyze` already do this for their headers or keys. Also, `handle_vm_timeline` does this.
3. **Change**: Update the `set_header` call in `handle_vm_list` to add bold attributes to each cell.
   - Currently: `let _ = table.set_header(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]);`
   - Should be:
     ```rust
            let _ = table.set_header([
                Cell::new("ID").add_attribute(comfy_table::Attribute::Bold),
                Cell::new("State").add_attribute(comfy_table::Attribute::Bold),
                Cell::new("RAM (MiB)").add_attribute(comfy_table::Attribute::Bold),
                Cell::new("CPUs").add_attribute(comfy_table::Attribute::Bold),
                Cell::new("Exit Reason").add_attribute(comfy_table::Attribute::Bold),
            ]);
     ```
4. **Verification**: After applying the change, use `cargo clippy` and `cargo test` to ensure there are no compilation errors or test failures. Remember to exclude the `wintun` dependents to run properly on linux.
5. **Submit**: Once verified, prepare PR details with title and description as specified for Mosaic.
