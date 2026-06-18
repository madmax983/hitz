🖌️ **Before:**
- Dashboard errors were appended linearly into a static top-level navigation text block (`Hitz VM Dashboard | Polling GET /vms | Error: Failed to parse JSON`), obscuring visual boundaries.
- Various `hitz vm dashboard` and `hitz vm top` interactive TUI tables used non-uniform header colors (such as `Color::Yellow` or plain unstyled text), which reduced overall CLI cohesion.

✨ **After:**
- Dashboard API or parsing errors are now isolated inside their own, clearly demarcated `Paragraph` widget with a deep red border and bold title `Error`. The top header bar strictly retains static navigation details.
- Standardized TUI data table headers (like "Disks", "Networks", "Top Processes", and the "Virtual Machines" list) to use a uniform Cyan bold modifier (`Color::Cyan` + `Modifier::BOLD`) matching the `comfy-table` configurations found elsewhere.

🖼️ **Visuals:**
If the hitz-daemon goes down while `hitz vm dashboard` is open, instead of blending into the top bar, a brand new bright red box drops down explaining the connection failure, pushing the (now stale) VM table safely down below it.
