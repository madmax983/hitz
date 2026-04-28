🎨 **Before:**
Running `hitz run --verbose` dumps a list of unstructured variables, using standard unformatted `eprintln!` commands that look like raw debugging statements.

✨ **After:**
The `hitz run` command with `--verbose` now uses `comfy_table` to output a well-structured layout showing the exact configuration parameters that the micro-VM will boot with. A nice cyan banner `🚀 Booting standalone micro-VM` was also added to clearly signal action, aligning with the Mosaic design philosophy.

🖼️ **Visuals:**
We replaced a stream of debug text with a clearly-formatted `Table` output matching `hitz vm status` formats and utilizing cyan/bold styles to structure data.
