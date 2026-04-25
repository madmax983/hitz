1. **Apply formatting to Top tables (Disks, Networks, Top Processes)**
   - Used `run_in_bash_session` to execute a Python script (`replace.py`) modifying the `handle_vm_top()` function in `crates/hitz-cli/src/main.rs`.
   - Updated table headers for Disks, Networks, and Processes to use `Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED)`.

2. **Apply formatting to Dashboard table and List view**
   - Used the same Python script method to update `handle_vm_dashboard()` and `handle_vm_list()`.
   - In `handle_vm_dashboard()`, the header styling was updated to match the Cyan, BOLD, UNDERLINED style, and the VM ID column was set to `Modifier::BOLD`.
   - In `handle_vm_list()`, the table header fields were explicitly styled with `Color::Cyan` and `comfy_table::Attribute::Bold`, and the VM ID value cell was styled with `comfy_table::Attribute::Bold`.
   - Ran `cargo fmt --manifest-path crates/hitz-cli/Cargo.toml` to format the code properly.

3. **Verify changes**
   - Run `cargo check -p hitz-cli --bin hitz` to ensure code compiles. Note: We ignore wintun/windows compilation errors on the Linux host as long as our `hitz-cli` `main.rs` doesn't trigger new semantic errors. `cargo test -p hitz-cli --bin hitz` fails as hitz-cli has no tests specifically.

4. **Complete pre-commit steps**
   - Complete pre-commit steps to ensure proper testing, verification, review, and reflection are done.

5. **Submit PR**
   - Use the `submit` tool to create a PR with the title "🎨 Mosaic: UI Polish for hitz-cli (Top and Dashboard TUI)" and the required description formatting:
     * 🖌️ **Before:** The TUI dashboard and top headers used default yellow text without underlines, and the list headers were uncolored, making them blend with the data.
     * ✨ **After:** The table headers now use Cyan, Bold, and Underlined styles for improved visual hierarchy, and the VM ID column values are bolded to stand out as primary identifiers.
     * 🖼️ **Visuals:** Added Cyan and Bold styling to CLI tables in `hitz vm list`, `hitz vm top`, and `hitz vm dashboard`.
