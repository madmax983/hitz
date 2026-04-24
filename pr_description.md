# 👺 Havoc: Fix lost wakeup race condition in SerialBuf

🧨 **The Trigger:**
A reader starts polling the `notified()` future in `SerialReader::read_chunk()`. The lock is released. Immediately after the lock drops but *before* the `.await` point is reached, the writer thread executes `write()`, advancing the buffer and invoking `notify.notify_waiters()`. Since the reader hasn't officially parked yet via `.await`, the wakeup notification is entirely discarded.

📉 **The Stack Trace:**
```
(No panic, but rather a permanent test hang / deadlock resulting in timeout):
thread 'tests::havoc_test_notify_race_condition' panicked at 'Havoc expected the data, but got deadlock/timeout!'
```

🧪 **Reproduction:**
Run `RUSTFLAGS="--cfg loom" cargo test -p hitz-vmm serial_buf`.

😈 **Comment:**
"You assumed `tokio::sync::Notify::notify_waiters()` would generously remember your intentions. You were wrong. It only cares about those who are already sleeping. By the time you went to sleep, the train had already left the station."
