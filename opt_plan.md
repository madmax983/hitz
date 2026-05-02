1. **Analyze `SerialReader::read_chunk` optimization:**
   - Currently, `SerialReader::read_chunk` in `crates/hitz-vmm/src/serial_buf.rs` uses a loop to wait for data. Inside the loop, it does `let mut rx = self.notify_rx.clone();` and then `let _ = rx.changed().await;`.
   - Cloning a `tokio::sync::watch::Receiver` increments an atomic reference count on the internal shared state, which has measurable overhead. Doing this inside an async loop on a hot path (reading serial output stream) is an unnecessary allocation-like atomic operation.
   - We can simply use `self.notify_rx.changed().await` instead. Because `changed()` requires `&mut self`, we can just make `notify_rx` mutable if it isn't already, or make `self`'s borrow of it mutable. Since `read_chunk` already takes `&mut self`, we can just call `self.notify_rx.changed().await`.
   - *Wait*, `tokio::sync::watch::Receiver::changed` updates its internal `seen` version. It works correctly when reused in a loop. In fact, that's the intended usage! Re-cloning it on every loop iteration actually resets the "seen" version to the *current* version, potentially missing intermediate changes or just adding overhead. The memory instruction `In Rust performance optimization, avoid cloning tokio::sync::watch::Receiver inside an async loop to call changed().await. Instead, use a mutable reference (&mut receiver) directly. This safely updates the internal 'seen' version and avoids unnecessary atomic reference counting overhead on every iteration.` explicitly confirms this.
   - We will remove the `.clone()` inside the loop in `SerialReader::read_chunk`.

2. **Verify changes using testing:**
   - Ensure `cargo test -p hitz-vmm` passes.
   - This change removes an unnecessary atomic operation on the async read loop path.

3. **Complete pre-commit steps to ensure proper testing, verification, review, and reflection are done.**

4. **Submit PR via Bolt persona.**
