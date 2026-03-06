# Phase 9: Graceful Shutdown

## Goal

Make VM shutdown deterministic. Ctrl+C stops all vCPUs immediately (via WHP cancel), the daemon drains in-flight work, and the process exits cleanly — no hung threads, no orphaned resources.

## Architecture

Three layers: (1) vCPU cancellation via HAL cancel handles, (2) daemon shutdown coordination with completion channels and listener tokens, (3) timeout + panic safety nets.

## vCPU Cancellation

### Problem

`vcpu.run()` blocks until the guest exits (I/O, HLT, etc.). Setting `stop_flag` only works if the run loop reaches its top-of-loop check. A halted vCPU stays blocked forever.

### Solution: Cancel Handles

Add a `CancelHandle` associated type to the `Vcpu` trait:

```rust
pub trait Vcpu: Send + 'static {
    type CancelHandle: Send + Sync + Clone;
    fn cancel_handle(&self) -> Self::CancelHandle;
    fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError>;
    // ... existing methods
}
```

For WHP, the cancel handle is `(Arc<PartitionInner>, VcpuId)` — the same data `cancel()` already uses. `cancel_via` calls `WHvCancelRunVirtualProcessor`, which forces `vcpu.run()` to return immediately.

### Multi-vCPU Flow

Before spawning threads, collect all cancel handles into `Arc<Vec<CancelHandle>>`. Each vCPU thread gets a clone. When a thread sets the stop flag, it also iterates all cancel handles and calls `cancel_via()`. This unblocks halted vCPUs within microseconds.

Single-vCPU path is unchanged — the stop flag check at loop top suffices.

## Exit Coordination Changes (vm.rs)

### Channel-Based Join with Timeout

Replace `handle.join()` loop with a bounded-time join:

1. Each vCPU thread sends its `Result<ExitReason, HalError>` on an `mpsc::Sender` before exiting.
2. Main thread calls `rx.recv_timeout(Duration::from_secs(3))` N times.
3. If any thread doesn't respond within 3 seconds after cancel, log it and proceed (thread is abandoned — it detaches when the handle drops).

### Panic Handling

Wrap the vCPU thread closure in `std::panic::catch_unwind`:
- On panic: set stop flag, cancel all vCPUs, record "vCPU panicked: {msg}" as exit reason.
- Don't re-panic. The thread exits cleanly, the mutex stays unpoisoned, other threads shut down normally.

## Daemon Shutdown Coordination

### Completion Channel

Replace fire-and-forget VM spawning with a completion channel:

- `VmManager` holds a `tokio::sync::mpsc::UnboundedSender<(String, VmState)>`.
- When `boot_and_run` finishes (in the spawned task), it sends `(vm_id, final_state)`.
- `stop_all_and_wait(&self, timeout: Duration)` sets stop flags, cancels vCPUs, then awaits the channel until all running VMs are accounted for or timeout expires.

### VmEntry Cancel Handles

`VmEntry` gains `cancel_handles: Option<Vec<CancelHandle>>` — set when the VM starts, cleared when it stops. `stop_all_and_wait` calls `cancel_via` on every stored handle.

### Listener Shutdown

Replace infinite `loop {}` in `run_pipe_listener` and `run_tcp_listener` with `tokio::select!` on a `CancellationToken`:

```rust
loop {
    tokio::select! {
        conn = listener.accept() => { /* handle */ }
        _ = token.cancelled() => break,
    }
}
```

In-flight HTTP requests finish naturally (hyper handles connection lifecycle). No new connections accepted after the token fires.

### Shutdown Sequence

1. Ctrl+C → fire cancellation token → listeners stop accepting
2. `stop_all_and_wait(5s)` → set flags + cancel vCPUs → wait for completion
3. Log any VMs that didn't stop in time
4. Drop remaining resources → exit

## Files Changed

| File | Action | Purpose |
|------|--------|---------|
| `hitz-hal/src/traits.rs` | Modify | Add `CancelHandle`, `cancel_handle()`, `cancel_via()` to Vcpu |
| `hitz-whp/src/vcpu.rs` | Modify | Implement CancelHandle as `(Arc<PartitionInner>, VcpuId)` |
| `hitz-vmm/src/vm.rs` | Modify | Collect cancel handles, cancel on exit, channel join, catch_unwind |
| `hitz-vmm/src/run_loop.rs` | Modify | Handle `Canceled` exit from WHP cancel (new VcpuExit variant?) |
| `hitz-daemon/src/vm_manager.rs` | Modify | Completion channel, cancel handle storage, `stop_all_and_wait` |
| `hitz-daemon/src/server.rs` | Modify | CancellationToken for listeners |
| `hitz-cli/src/main.rs` | Modify | Call cancel on Ctrl+C for standalone `hitz run` |
| `hitz-whp/src/tests.rs` | Modify | Phase 9 integration tests |

## Testing

**Unit tests:**
- CancelHandle construction and cancel_via (hitz-whp)
- stop_all_and_wait with FakeHypervisor (hitz-daemon)
- Panic recovery: simulated panicking vCPU thread (hitz-vmm)
- Timeout: abandoned thread doesn't block caller (hitz-vmm)

**Integration tests (#[ignore]):**
- `phase9_cancel_halted_vcpu`: 2-vCPU, BSP halts, cancel APs, verify clean exit < 1s
- `phase9_daemon_graceful_shutdown`: start daemon + VM, trigger shutdown, verify VM stops before exit

## What We're NOT Building

- No VM pause/resume (checkpoint/restore)
- No persistent VM registry (disk state)
- No daemon restart recovery
- No listener TLS/authentication
- No per-VM resource limits
