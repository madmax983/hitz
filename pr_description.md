🎨 Mosaic: UI Polish for hitz-cli

🖌️ **Before:** The VM list dashboard table (`handle_vm_list` in `hitz-cli/src/main.rs`) did not have bolded text in its headers, failing to establish visual hierarchy and making it inconsistent with the rest of the CLI tools.
✨ **After:** Added bold formatting to the "ID", "State", "RAM (MiB)", "CPUs", and "Exit Reason" cells using `comfy_table::Attribute::Bold`.
🖼️ **Visuals:** The `hitz vm list` command now features clearly styled headers that cleanly separate the metadata column titles from the dynamic data values, satisfying the 3-Click Rule for quick dashboard scanning.
