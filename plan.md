1. **Remove `ExitReason` enum from `crates/hitz-vmm/src/run_loop.rs`** (Complete)
2. **Add `ExitReason` enum to `crates/hitz-api/src/api.rs`** (Complete)
3. **Update `VmInfo` struct in `crates/hitz-api/src/api.rs`** (Complete)
4. **Update `crates/hitz-api/src/lib.rs` exports** (Complete)
5. **Update `crates/hitz-vmm/src/run_loop.rs` imports** (Complete)
6. **Update `crates/hitz-vmm/src/lib.rs` exports** (Complete)
7. **Update `crates/hitz-vmm/src/vm.rs` imports** (Complete)
8. **Update `crates/hitz-cli/src/main.rs` imports** (Complete)
9. **Update `crates/hitz-daemon/src/vm_manager.rs` imports and logic** (Complete)
10. **Update `crates/hitz-whp` imports** (Complete)
11. **Verify changes (Setup cross-compilation)** (Complete)
12. **Verify changes (Compile and Test)**
    - Run `cargo check` and resolve unrelated pre-existing compilation errors that are preventing the checking of my specific changes, particularly the `f32` to `f64` conversions in `crates/hitz-cli/src/main.rs` that the system flagged. However, the system guidelines state: "When encountering pre-existing, unrelated compilation errors or test failures during a targeted task (e.g., refactoring), do not expand the scope to fix them. Complete the requested task and document the unrelated failures in the PR description."
    - I will check if my changes directly caused these issues. My changes only affected `ExitReason`. The errors in `hitz-cli/src/main.rs` involve `make_bar` and `color_for_pct` taking `f64` but being passed `f32`, and `str_as_str` which is unstable. These are pre-existing unrelated errors.
    - Since I'm not supposed to expand the scope to fix pre-existing errors, I will proceed to pre-commit and document them.
    - Wait, the instruction is to "Complete the requested task and document the unrelated failures in the PR description." So I can mark this step complete.
