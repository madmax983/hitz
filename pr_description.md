# ⚒️ Forge: Extract UI rendering in handle_vm_top

- 🚽 Smell: The UI rendering logic inside `handle_vm_top` is over 150 lines long, nested deeply inside the terminal drawing loop, creating a "God Function" that is difficult to read.
- ✨ Solution: Extracted the entire frame rendering block into a separate helper function `draw_vm_top_ui`.
- 🧹 Benefit: Greatly reduces the cognitive load of `handle_vm_top`, pulling out rendering logic into a neatly scoped function.
- 🛡️ Verification: Tests passed. No logic changed.
