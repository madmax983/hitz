🎯 Target: hitz-vmm/src/run_loop.rs (specifically `poll_devices` and `dispatch_exit` for Canceled)
💣 Risk: Ensures the VM run loop gracefully handles a poisoned lock over the shared devices structure without deadlocking or silently proceeding.
🧪 Strategy: Added two `#[should_panic(expected = "device lock poisoned")]` unit tests (`should_panic_on_poll_devices_with_poisoned_lock` and `should_panic_on_dispatch_exit_canceled_with_poisoned_lock`) that artificially poison the `devices` Mutex before passing it to the target functions.
🔬 Verification: Run `cargo test -p hitz-vmm --lib --no-default-features --target x86_64-unknown-linux-gnu`
