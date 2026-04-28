⚒️ Forge: Refactor hitz-daemon state_store and router to flatten nested match blocks

**🛁 Smell**
Deeply nested `match` statements were present when routing HTTP paths in `router.rs` and when reading directories in `state_store.rs`, leading to a Pyramid of Doom pattern that hurt readability and increased cognitive load.

**✨ Solution**
- In `state_store.rs`, I flattened the nested match blocks for `std::fs::read_dir` and the directory loop iterations by using `let Ok(read_dir) = std::fs::read_dir(...) else` guard clauses, combined with `inspect_err` for concise logging.
- In `router.rs`, I refactored the `route` and `route_vm` functions to replace the complex nesting of `match (&method, path.as_str())` and `match segments.nth(2)` with sequential `if` conditions and guard clauses with early returns.
- `vm_manager.rs` was verified to ensure its `match` nesting inside `create_vm`'s `spawn_blocking` handler was fully flattened via variable decomposition.

**🧹 Benefit**
Drastically reduces nesting and mental overhead. The code now follows idiomatic Rust early-return and guard-clause patterns, making execution paths linear and straightforward to comprehend.

**🛡️ Verification**
Tests passed (for crates compiling on linux target). No logic or behavior was changed, only control-flow flattening. (Note: `hitz-daemon` failed to fully link the test binary on Linux purely due to the expected transitive `wintun` crate platform limits, but source validation and checking completed successfully).
