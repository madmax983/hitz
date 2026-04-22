💡 **The Spark:** I noticed we capture detailed resource metrics across VMs but lack a quick, quantifiable way to identify similarly behaving VMs at a glance.

🚀 **The Feature:** Implemented `VmFingerprint` generation in the new `fingerprint` module. It quantizes a continuous `MetricsSnapshot` into a discrete, deterministic identifier (e.g. `FP-C9-R4-D0-N1`) using percentage bucketing for CPU/RAM and logarithmic magnitude bucketing for Disk/Network I/O.

🔭 **The Potential:** This enables rapid clustering, pattern matching, and caching. We could easily group "Compute Heavy" or "Network Heavy" nodes based purely on their structural fingerprint, instantly spotting anomalies or noisy neighbors without deep-diving into the raw metrics stream.

⚠️ **Risk:** Low. The module is fully isolated in `src/fingerprint.rs` and placed behind a `fingerprint` cargo feature flag, avoiding any impact to core hypervisor logic or existing consumers.
🎯 Target: hitz-vmm/src/run_loop.rs (specifically `poll_devices` and `dispatch_exit` for Canceled)
💣 Risk: Ensures the VM run loop gracefully handles a poisoned lock over the shared devices structure without deadlocking or silently proceeding.
🧪 Strategy: Added two `#[should_panic(expected = "device lock poisoned")]` unit tests (`should_panic_on_poll_devices_with_poisoned_lock` and `should_panic_on_dispatch_exit_canceled_with_poisoned_lock`) that artificially poison the `devices` Mutex before passing it to the target functions.
🔬 Verification: Run `cargo test -p hitz-vmm --lib --no-default-features --target x86_64-unknown-linux-gnu`
