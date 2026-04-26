⚒️ Forge: Refactor to fix readibility smells.

🚮 Smell: `hitz-daemon`'s `port_forward.rs` had `match listener.accept()` inside `loop`, deeply nesting the successful connection handling logic and violating the "Pyramid of Doom" smell rule.
✨ Solution: Inverted `listener.accept()` to `let Ok(...) = match listener.accept().await { ... } else { continue; }` in `port_forward.rs` (using early return logic) to remove one level of nesting while still correctly handling and logging the error payload `e`. Added `.map(|_| None)` to `VcpuExit` processing in `run_loop.rs` to remove intermediate `Ok(None)` results and flatten `match exit` slightly without modifying semantics.
🧼 Benefit: Reduces cognitive load and strictly enforces flatter control flow avoiding Pyramids of Doom without altering runtime execution or observability.
🛡️ Verification: Tests passed. No logic changed. Error payloads preserved.
