# Phase 9: Graceful Shutdown — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make VM shutdown deterministic — Ctrl+C stops all vCPUs immediately, the daemon drains in-flight work, and the process exits cleanly.

**Architecture:** Three layers: (1) watchdog thread inside `boot_and_run` that polls `stop_flag` and calls `cancel_via()` to force halted vCPUs out of `vcpu.run()`; (2) daemon completion channel so `stop_all_and_wait` knows when VMs have actually stopped; (3) `tokio::sync::watch` shutdown signal for listener loops.

**Tech Stack:** Rust 2024, WHP `WHvCancelRunVirtualProcessor`, `std::panic::catch_unwind`, `std::sync::mpsc` (thread join), `tokio::sync::mpsc` (completion), `tokio::sync::watch` (listener shutdown).

---

## Task 1: CancelHandle HAL Trait

Add `CancelHandle` associated type and methods to the `Vcpu` trait so cancel handles can be extracted before thread spawn and called from any thread.

**File:** `crates/hitz-hal/src/traits.rs`

**Changes:**

Add three items to the `Vcpu` trait (after the existing `cancel` method):

```rust
/// Opaque handle that can cancel a running vCPU from any thread.
type CancelHandle: Send + Sync + Clone;

/// Extract a cancel handle from this vCPU.
///
/// Call this before moving the vCPU into a thread. The handle can
/// be cloned and shared freely.
fn cancel_handle(&self) -> Self::CancelHandle;

/// Cancel a running vCPU using a previously extracted handle.
///
/// This causes `run` to return `VcpuExit::Canceled`.
fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError>;
```

Give `cancel` a default implementation that delegates:

```rust
/// Cancel a running vCPU from another thread.
///
/// This causes `run` to return `VcpuExit::Canceled`.
fn cancel(&self) -> Result<(), HalError> {
    Self::cancel_via(&self.cancel_handle())
}
```

**Verification:**

```
cargo check -p hitz-hal
```

Expected: fails (downstream impls not yet updated).

---

## Task 2: WHP CancelHandle Implementation

Implement `CancelHandle` for `WhpVcpu` using the same `(Arc<PartitionInner>, u32)` data that `cancel()` already uses.

**File:** `crates/hitz-whp/src/vcpu.rs`

**Changes:**

Add a public newtype struct (above `impl Vcpu for WhpVcpu`):

```rust
/// Opaque cancel handle for a WHP virtual processor.
///
/// Holds the partition handle and vCPU index needed to call
/// `WHvCancelRunVirtualProcessor` from any thread.
#[derive(Clone)]
pub struct WhpCancelHandle {
    partition: Arc<PartitionInner>,
    index: u32,
}

// SAFETY: PartitionInner.handle is a raw Windows handle that is safe to send
// across threads (WHP API is thread-safe for cancel operations).
// These impls are required because Arc<PartitionInner> contains a raw HANDLE.
unsafe impl Send for WhpCancelHandle {}
unsafe impl Sync for WhpCancelHandle {}
```

Add to `impl Vcpu for WhpVcpu`:

```rust
type CancelHandle = WhpCancelHandle;

fn cancel_handle(&self) -> Self::CancelHandle {
    WhpCancelHandle {
        partition: self.partition.clone(),
        index: self.index,
    }
}

fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError> {
    // SAFETY: partition handle is valid, flags must be 0.
    unsafe { WHvCancelRunVirtualProcessor(handle.partition.handle, handle.index, 0) }
        .map_err(|e| HalError::VcpuCancel(format!("WHvCancelRunVirtualProcessor: {e}")))
}
```

Remove the manual `cancel` implementation (the default trait impl delegates to `cancel_via`).

**Verification:**

```
cargo check -p hitz-whp
```

Expected: fails (hitz-vmm and hitz-daemon FakeVcpu not yet updated).

---

## Task 3: vm.rs Watchdog + catch_unwind + Channel Join

The core shutdown logic. Three changes to `boot_and_run`:

1. **Watchdog thread** — polls `stop_flag`, calls `cancel_via` on all vCPUs when triggered
2. **catch_unwind** — wraps vCPU thread closures so panics don't cascade
3. **Channel-based join** — replaces `handle.join()` with `mpsc::recv_timeout`

**File:** `crates/hitz-vmm/src/vm.rs`

### 3a. Add imports

At the top, add:

```rust
use std::sync::mpsc;
use std::time::{Duration, Instant};
```

And add `Partition` to the `hitz_hal` import:

```rust
use hitz_hal::{
    Gpa, GuestMemAccess, Hypervisor, MemFlags, MemSizeMiB, Partition, PartitionConfig, Vcpu,
    VcpuId,
};
```

### 3b. Add cancel helper function

Above `boot_and_run`, add:

```rust
/// Cancel all vCPUs using their extracted handles.
fn cancel_all_vcpus<V: Vcpu>(handles: &[V::CancelHandle]) {
    for h in handles {
        let _ = V::cancel_via(h);
    }
}
```

### 3c. Single-vCPU path: add watchdog

Replace the single-vCPU block (lines 324-330) with:

```rust
if vcpus.len() == 1 {
    // Watchdog: polls stop_flag and cancels the vCPU if set.
    // This ensures Ctrl+C works even when the guest is halted.
    let cancel_handle = vcpus[0].cancel_handle();
    let stop_clone = stop_flag.clone();
    let watchdog = std::thread::Builder::new()
        .name("cancel-watchdog".into())
        .spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = <<H::Partition as Partition>::Vcpu>::cancel_via(&cancel_handle);
        })
        .expect("spawn watchdog");

    let exit_reason =
        run_loop::run_vcpu_loop(&mut vcpus[0], &devices, &*guest_mem_arc, &stop_flag)?;

    // Signal watchdog to stop (may already be stopped).
    stop_flag.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    drop(net_io_handle);
    return Ok(VmRunResult { exit_reason });
}
```

### 3d. Multi-vCPU path: watchdog + cancel handles + catch_unwind + channel

Replace the entire multi-vCPU block (lines 332-386) with:

```rust
// Multi-vCPU: spawn a thread per vCPU.
let cancel_handles: Vec<_> = vcpus.iter().map(Vcpu::cancel_handle).collect();
let shared_handles = Arc::new(cancel_handles);
let (exit_tx, exit_rx) = mpsc::channel();
let num_vcpus = vcpus.len();

// Watchdog thread: ensures cancel fires even if all vCPUs are blocked.
let watchdog = {
    let stop_clone = stop_flag.clone();
    let handles_clone = shared_handles.clone();
    std::thread::Builder::new()
        .name("cancel-watchdog".into())
        .spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&handles_clone);
        })
        .expect("spawn watchdog")
};

let handles: Vec<_> = vcpus
    .into_iter()
    .enumerate()
    .map(|(idx, mut vcpu)| {
        let devs = devices.clone();
        let mem = guest_mem_arc.clone();
        let stop = stop_flag.clone();
        let cancel_handles = shared_handles.clone();
        let tx = exit_tx.clone();

        std::thread::Builder::new()
            .name(format!("vcpu-{idx}"))
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_loop::run_vcpu_loop(&mut vcpu, &devs, &*mem, &stop)
                    }));

                // On terminal exit or panic: stop all vCPUs.
                match &result {
                    Ok(Ok(
                        ExitReason::Halt
                        | ExitReason::Shutdown
                        | ExitReason::Unexpected(_),
                    ))
                    | Ok(Err(_))
                    | Err(_) => {
                        stop.store(true, Ordering::Relaxed);
                        cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(
                            &cancel_handles,
                        );
                    }
                    Ok(Ok(ExitReason::Canceled)) => {
                        // Another vCPU already triggered stop.
                    }
                }

                // Convert result to ExitReason.
                let exit = match result {
                    Ok(r) => r,
                    Err(payload) => {
                        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                            (*s).to_string()
                        } else if let Some(s) = payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        Ok(ExitReason::Unexpected(format!("vCPU {idx} panicked: {msg}")))
                    }
                };

                let _ = tx.send(exit);
            })
            .expect("spawn vcpu thread")
    })
    .collect();

// Drop our copy of the sender so recv knows when all threads are done.
drop(exit_tx);

// Collect results with timeout.
let deadline = Instant::now() + Duration::from_secs(3);
let mut final_reason = ExitReason::Canceled;

for _ in 0..num_vcpus {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match exit_rx.recv_timeout(remaining) {
        Ok(Ok(reason)) => {
            if final_reason == ExitReason::Canceled {
                final_reason = reason;
            }
        }
        Ok(Err(e)) => {
            if final_reason == ExitReason::Canceled {
                // HAL error from a vCPU — report as unexpected.
                final_reason =
                    ExitReason::Unexpected(format!("vCPU error: {e}"));
            }
        }
        Err(_) => {
            tracing::warn!("vCPU thread did not respond within timeout");
            break;
        }
    }
}

// Ensure watchdog exits.
stop_flag.store(true, Ordering::Relaxed);
let _ = watchdog.join();

// Join vCPU threads (should be near-instant since they already sent results).
for handle in handles {
    let _ = handle.join();
}

drop(net_io_handle);
Ok(VmRunResult {
    exit_reason: final_reason,
})
```

### 3e. Remove `first_exit` mutex

The `first_exit: Arc<Mutex<Option<ExitReason>>>` (line 333) and its usage (lines 378-381) are replaced by the channel approach. Delete:
- `let first_exit: Arc<Mutex<Option<ExitReason>>>` line
- The `first_exit.clone()` in thread setup
- The `captured_reason` block at the end

**Verification:**

```
cargo check -p hitz-vmm
```

Expected: compiles. Then:

```
cargo test -p hitz-vmm
```

Expected: all existing tests pass (validate_config tests don't exercise multi-vCPU).

---

## Task 4: Daemon Completion Channel + stop_all_and_wait

Add a completion channel so the daemon knows when VMs actually stop. Update `stop_all` to `stop_all_and_wait` with timeout. Update `FakeVcpu` to implement the new trait.

**File:** `crates/hitz-daemon/src/vm_manager.rs`

### 4a. Update FakeVcpu in tests

At the bottom of the test module, add to `impl hitz_hal::Vcpu for FakeVcpu`:

```rust
type CancelHandle = ();

fn cancel_handle(&self) -> Self::CancelHandle {}

fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), hitz_hal::HalError> {
    Ok(())
}
```

And remove the existing `cancel` method (the default impl delegates to `cancel_via`):

```rust
// DELETE:
fn cancel(&self) -> Result<(), hitz_hal::HalError> {
    Ok(())
}
```

### 4b. Add completion channel to VmManager

Add imports at the top:

```rust
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
```

Add fields to `VmManager`:

```rust
pub struct VmManager<H> {
    hypervisor: Arc<H>,
    vms: Arc<Mutex<HashMap<String, VmEntry>>>,
    completion_tx: tokio_mpsc::UnboundedSender<String>,
    completion_rx: Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<String>>>,
}
```

Update `Clone` impl:

```rust
impl<H> Clone for VmManager<H> {
    fn clone(&self) -> Self {
        Self {
            hypervisor: self.hypervisor.clone(),
            vms: self.vms.clone(),
            completion_tx: self.completion_tx.clone(),
            completion_rx: self.completion_rx.clone(),
        }
    }
}
```

Update `new`:

```rust
pub fn new(hypervisor: Arc<H>) -> Self {
    let (tx, rx) = tokio_mpsc::unbounded_channel();
    Self {
        hypervisor,
        vms: Arc::new(Mutex::new(HashMap::new())),
        completion_tx: tx,
        completion_rx: Arc::new(tokio::sync::Mutex::new(rx)),
    }
}
```

### 4c. Send completion in start_vm

In `start_vm`, clone the completion_tx before the spawn:

```rust
let completion_tx = self.completion_tx.clone();
```

Inside the spawned task, after updating state and before the closing `}`:

```rust
let _ = completion_tx.send(vm_id);
```

### 4d. Replace stop_all with stop_all_and_wait

Replace the `stop_all` method with:

```rust
/// Stop all running VMs and wait for them to finish.
///
/// Sets stop flags for all running VMs (triggering their internal
/// watchdog threads to cancel vCPUs), then waits on the completion
/// channel until all VMs report done or the timeout expires.
pub async fn stop_all_and_wait(&self, timeout: Duration) {
    let running_count = {
        let Ok(vms) = self.vms.lock() else { return };
        let mut count = 0usize;
        for entry in vms.values() {
            if entry.state == VmState::Running {
                if let Some(ref flag) = entry.stop_flag {
                    flag.store(true, Ordering::Relaxed);
                }
                count += 1;
            }
        }
        count
    };

    if running_count == 0 {
        return;
    }

    tracing::info!(running_count, "waiting for VMs to stop");

    let deadline = tokio::time::Instant::now() + timeout;
    let mut rx = self.completion_rx.lock().await;
    let mut stopped = 0usize;

    while stopped < running_count {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(_vm_id)) => {
                stopped += 1;
            }
            Ok(None) => break, // channel closed
            Err(_) => {
                tracing::warn!(
                    remaining = running_count - stopped,
                    "timeout waiting for VMs to stop"
                );
                break;
            }
        }
    }
}
```

Also keep a sync `stop_all` for non-async contexts:

```rust
/// Stop all running VMs (fire-and-forget, does not wait).
pub fn stop_all(&self) {
    if let Ok(vms) = self.vms.lock() {
        for entry in vms.values() {
            if entry.state == VmState::Running {
                if let Some(ref flag) = entry.stop_flag {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}
```

**Verification:**

```
cargo test -p hitz-daemon
```

Expected: all daemon tests pass (FakeVcpu updated, completion channel created in `make_manager`).

---

## Task 5: Listener Shutdown + CLI Daemon Update

Replace infinite listener loops with `tokio::select!` on a `watch` channel. Update the CLI's `run_daemon` to use `stop_all_and_wait`.

### 5a. server.rs — add shutdown parameter

**File:** `crates/hitz-daemon/src/server.rs`

Replace `run_server` signature:

```rust
pub async fn run_server<H>(
    pipe_path: &str,
    tcp_addr: Option<SocketAddr>,
    manager: VmManager<H>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    if let Some(addr) = tcp_addr {
        let mgr = manager.clone();
        let mut sd = shutdown.clone();
        drop(tokio::spawn(async move {
            if let Err(e) = run_tcp_listener(addr, mgr, &mut sd).await {
                tracing::error!("TCP listener error: {e}");
            }
        }));
    }

    run_pipe_listener(pipe_path, manager, &mut shutdown).await
}
```

Replace `run_pipe_listener`:

```rust
async fn run_pipe_listener<H>(
    pipe_path: &str,
    manager: VmManager<H>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let mut first = true;

    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(first)
            .create(pipe_path)?;
        first = false;

        // Wait for a client or shutdown signal.
        tokio::select! {
            result = server.connect() => {
                result?;
                tracing::debug!("client connected to {pipe_path}");

                let mgr = manager.clone();
                drop(tokio::spawn(async move {
                    let io = TokioIo::new(server);
                    let service = service_fn(move |req| {
                        let m = mgr.clone();
                        async move { router::route(req, &m).await }
                    });

                    if let Err(e) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await
                    {
                        tracing::error!("pipe connection error: {e}");
                    }
                }));
            }
            _ = shutdown.changed() => {
                tracing::info!("pipe listener shutting down");
                break;
            }
        }
    }
    Ok(())
}
```

Replace `run_tcp_listener`:

```rust
async fn run_tcp_listener<H>(
    addr: SocketAddr,
    manager: VmManager<H>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("TCP listener bound to {addr}");

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, peer) = result?;
                tracing::debug!("TCP client connected from {peer}");

                let mgr = manager.clone();
                drop(tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let service = service_fn(move |req| {
                        let m = mgr.clone();
                        async move { router::route(req, &m).await }
                    });

                    if let Err(e) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await
                    {
                        tracing::error!("TCP connection error ({peer}): {e}");
                    }
                }));
            }
            _ = shutdown.changed() => {
                tracing::info!("TCP listener shutting down");
                break;
            }
        }
    }
    Ok(())
}
```

### 5b. CLI run_daemon — use shutdown signal + stop_all_and_wait

**File:** `crates/hitz-cli/src/main.rs`

Replace the `run_daemon` function:

```rust
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("hitz=debug")
            .with_writer(std::io::stderr)
            .init();
    }

    eprintln!("hitz: daemon listening on {}", args.pipe);
    if let Some(addr) = args.tcp_listen {
        eprintln!("hitz: TCP listener on {addr}");
    }

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().context("WHP not available")?);
        let manager = hitz_daemon::VmManager::new(hv);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let server_mgr = manager.clone();
        let pipe = args.pipe.clone();
        let tcp = args.tcp_listen;
        let server_handle = tokio::spawn(async move {
            hitz_daemon::run_server(&pipe, tcp, server_mgr, shutdown_rx).await
        });

        // Wait for Ctrl+C.
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl+C")?;
        eprintln!("\nhitz: shutting down...");

        // Signal listeners to stop accepting.
        let _ = shutdown_tx.send(true);

        // Drain VMs with 5-second timeout.
        manager
            .stop_all_and_wait(std::time::Duration::from_secs(5))
            .await;

        // Wait for server to finish.
        let _ = server_handle.await;

        eprintln!("hitz: shutdown complete");
        Ok(())
    })
}
```

**Verification:**

```
cargo check --workspace
cargo test --workspace
```

Expected: all tests pass, no new warnings.

---

## Task 6: Integration Tests

Add WHP integration tests that verify the shutdown path works end-to-end.

**File:** `crates/hitz-whp/src/tests.rs`

### 6a. phase9_cancel_via_cancel_handle

Test that `cancel_via` works by canceling a running vCPU from another thread. Uses `boot_and_run` with the "Hello" ELF pattern (same as Phase 5) but sets the stop flag externally after a delay, exercising the watchdog cancel path.

```rust
/// Phase 9: boot_and_run exits cleanly when stop_flag is set externally.
///
/// Boots the "Hello" ELF, sets the stop flag after 200ms. The internal
/// watchdog thread detects the flag and calls cancel_via() on the vCPU,
/// forcing it out of vcpu.run(). Verifies boot_and_run returns within
/// a reasonable time.
#[test]
#[ignore]
fn phase9_cancel_via_stop_flag() {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    // x86-64: write "Hello" to COM1 then HLT — same as Phase 5.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();

    // Set stop flag after 200ms — watchdog should cancel within ~10ms.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        flag.store(true, Ordering::Relaxed);
    });

    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let result = hitz_vmm::boot_and_run(&hv, &config, writer, stop_flag)
        .expect("boot_and_run should succeed");

    // The ELF runs so fast it may HLT before the stop flag is set.
    // Either Halt or Canceled is acceptable.
    assert!(
        matches!(result.exit_reason, ExitReason::Halt | ExitReason::Canceled),
        "expected Halt or Canceled, got {:?}",
        result.exit_reason
    );

    // Serial output should still contain "Hello" regardless of exit path.
    let output = buffer.lock().expect("lock buffer");
    assert_eq!(
        output.as_slice(),
        b"Hello",
        "serial output mismatch: got {:?}",
        String::from_utf8_lossy(&output)
    );
}
```

### 6b. phase9_multi_vcpu_cancel

Test that multi-vCPU `boot_and_run` exits cleanly when the stop flag is set. This exercises the watchdog + cancel_all_vcpus path with multiple threads.

```rust
/// Phase 9: multi-vCPU boot_and_run exits cleanly on cancel.
///
/// Same "Hello" ELF with 2 vCPUs. BSP runs and HLTs; AP starts in
/// wait-for-SIPI. Setting stop_flag triggers the watchdog to cancel
/// both vCPUs. Verifies all threads exit cleanly.
#[test]
#[ignore]
fn phase9_multi_vcpu_cancel() {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x69, // mov al, 'i'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 2,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();

    // Cancel after 500ms.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        flag.store(true, Ordering::Relaxed);
    });

    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let start = std::time::Instant::now();
    let result = hitz_vmm::boot_and_run(&hv, &config, writer, stop_flag)
        .expect("boot_and_run should succeed");
    let elapsed = start.elapsed();

    assert!(
        matches!(result.exit_reason, ExitReason::Halt | ExitReason::Canceled),
        "expected Halt or Canceled, got {:?}",
        result.exit_reason
    );

    // Should complete within 2 seconds (500ms delay + watchdog + thread join).
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "boot_and_run took too long: {elapsed:?}"
    );
}
```

Note: Both tests reuse the `make_boot_elf` helper and `SharedWriter` struct already defined in the test module.

**Verification:**

```
cargo test -p hitz-whp -- --ignored --test-threads=1 phase9
```

Expected: both Phase 9 tests pass.

Then run the full suite:

```
cargo test --workspace
cargo test -p hitz-whp -- --ignored --test-threads=1
cargo clippy --workspace -- -D warnings
```

---

## Dependency Graph

```
Task 1 (HAL trait) ──► Task 2 (WHP impl) ──────────────────► Task 6 (tests)
                   ──► Task 3 (vm.rs watchdog/catch_unwind) ► Task 6 (tests)
                   ──► Task 4 (daemon completion/FakeVcpu)  ► Task 5 (listener shutdown)
```

Tasks 2 and 3 can be done in parallel after Task 1.
Task 4 must be done before Task 5 (stop_all_and_wait used by CLI).
Task 6 depends on Tasks 2+3 (WHP cancel handle + watchdog).

## Files Summary

| File | Action | Task |
|------|--------|------|
| `hitz-hal/src/traits.rs` | Modify | 1 |
| `hitz-whp/src/vcpu.rs` | Modify | 2 |
| `hitz-vmm/src/vm.rs` | Modify | 3 |
| `hitz-daemon/src/vm_manager.rs` | Modify | 4 |
| `hitz-daemon/src/server.rs` | Modify | 5 |
| `hitz-cli/src/main.rs` | Modify | 5 |
| `hitz-whp/src/tests.rs` | Modify | 6 |

## What We're NOT Building

- No VM pause/resume (checkpoint/restore)
- No persistent VM registry (disk state)
- No daemon restart recovery
- No listener TLS/authentication
- No per-VM resource limits
- No ACPI shutdown (virtio-powered graceful guest shutdown)

## Verification (Final)

1. `cargo fmt --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace` — existing + new tests
4. `cargo test -p hitz-whp -- --ignored --test-threads=1` — Phase 0–9 tests
5. Manual: `hitz daemon start -v`, create+start VM, Ctrl+C → verify clean exit within 5s
