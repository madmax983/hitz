# Phase 6: Daemon Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Named pipe daemon with multi-VM lifecycle management, plus Ctrl+C for `hitz run`.

**Architecture:** VmManager orchestrates VMs via `spawn_blocking(boot_and_run)`. Named pipe server speaks HTTP/1.1 via hyper. CLI gains `daemon start` and `vm *` subcommands. Stop flag (`AtomicBool`) threads through `boot_and_run` into `run_vcpu_loop`.

**Tech Stack:** tokio, hyper 1.x, named pipes (`tokio::net::windows::named_pipe`), serde_json, ctrlc, clap

---

## Task 1: API Types in `hitz-api`

**Files:**
- Modify: `crates/hitz-api/src/lib.rs`

### Step 1: Write the tests

Append the following tests inside the existing `mod tests` block in `crates/hitz-api/src/lib.rs`, before the closing `}`:

```rust
    #[test]
    fn vm_action_serde_roundtrip() {
        let start = VmAction::Start;
        let json = serde_json::to_string(&start).expect("serialize");
        let restored: VmAction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, VmAction::Start);

        let stop = VmAction::Stop;
        let json = serde_json::to_string(&stop).expect("serialize");
        let restored: VmAction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, VmAction::Stop);
    }

    #[test]
    fn vm_state_serde_roundtrip() {
        for state in [VmState::Created, VmState::Running, VmState::Stopped, VmState::Failed] {
            let json = serde_json::to_string(&state).expect("serialize");
            let restored: VmState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, state);
        }
    }

    #[test]
    fn vm_info_serde_roundtrip() {
        let info = VmInfo {
            id: "test-vm".into(),
            state: VmState::Running,
            config: VmConfig {
                kernel_path: PathBuf::from("vmlinux"),
                initramfs_path: None,
                disk_path: None,
                ram_mib: DEFAULT_RAM_MIB,
                cmdline: None,
            },
            exit_reason: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: VmInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.id, "test-vm");
        assert_eq!(restored.state, VmState::Running);
        assert!(restored.exit_reason.is_none());
    }

    #[test]
    fn create_vm_request_serde() {
        let req = CreateVmRequest {
            config: VmConfig {
                kernel_path: PathBuf::from("vmlinux"),
                initramfs_path: None,
                disk_path: None,
                ram_mib: 128,
                cmdline: None,
            },
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CreateVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.config.ram_mib, 128);
    }

    #[test]
    fn action_vm_request_serde() {
        let req = ActionVmRequest { action: VmAction::Start };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("Start"));
        let restored: ActionVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.action, VmAction::Start);
    }

    #[test]
    fn api_error_serde() {
        let err = ApiError { message: "not found".into() };
        let json = serde_json::to_string(&err).expect("serialize");
        let restored: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.message, "not found");
    }
```

### Step 2: Run tests — verify they fail

Run: `cargo test -p hitz-api 2>&1 | tail -5`
Expected: compilation errors (types don't exist yet)

### Step 3: Add the API types

Add the following after the `VmConfig` impl block (before `#[cfg(test)]`) in `crates/hitz-api/src/lib.rs`:

```rust
/// Action to perform on a VM via `POST /vms/{id}/action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmAction {
    /// Boot the VM.
    Start,
    /// Cancel the vCPU and stop the VM.
    Stop,
}

/// Current state of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmState {
    /// Config validated, not yet booted.
    Created,
    /// vCPU loop is running.
    Running,
    /// Exited cleanly (HLT or canceled).
    Stopped,
    /// Triple fault, unexpected exit, or boot error.
    Failed,
}

/// Body for `PUT /vms/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    /// VM configuration.
    pub config: VmConfig,
}

/// Body for `POST /vms/{id}/action`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionVmRequest {
    /// Action to perform.
    pub action: VmAction,
}

/// VM info returned by GET endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    /// VM identifier.
    pub id: String,
    /// Current state.
    pub state: VmState,
    /// Config used to create the VM.
    pub config: VmConfig,
    /// Exit reason (set when Stopped or Failed).
    pub exit_reason: Option<String>,
}

/// Generic API error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Human-readable error message.
    pub message: String,
}
```

### Step 4: Run tests — verify they pass

Run: `cargo test -p hitz-api`
Expected: all tests pass (11 total: 5 existing + 6 new)

### Step 5: Commit

```bash
git add crates/hitz-api/src/lib.rs
git commit -m "feat(api): add VmAction, VmState, VmInfo, and request/response types for daemon API"
```

---

## Task 2: Stop Flag + `ExitReason::Canceled`

**Files:**
- Modify: `crates/hitz-vmm/src/run_loop.rs`
- Modify: `crates/hitz-vmm/src/vm.rs`
- Modify: `crates/hitz-vmm/src/lib.rs`
- Modify: `crates/hitz-cli/src/main.rs`
- Modify: `crates/hitz-whp/src/tests.rs`

### Step 1: Add `Canceled` variant to `ExitReason`

In `crates/hitz-vmm/src/run_loop.rs`, add `Canceled` to the `ExitReason` enum:

```rust
/// Reason the run loop terminated.
#[derive(Debug, PartialEq, Eq)]
pub enum ExitReason {
    /// Guest executed HLT.
    Halt,
    /// Guest initiated shutdown (triple fault, etc.).
    Shutdown,
    /// VM was stopped via the stop flag (user-initiated cancel).
    Canceled,
    /// Unexpected exit that the run loop doesn't know how to handle.
    Unexpected(String),
}
```

### Step 2: Add `stop_flag` parameter to `run_vcpu_loop`

In `crates/hitz-vmm/src/run_loop.rs`:

1. Add import at the top:
```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

2. Change the `run_vcpu_loop` signature to accept a stop flag:
```rust
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    serial: &mut SerialDevice<W>,
    mmio_bus: &mut MmioBus,
    mem: &dyn GuestMemAccess,
    stop_flag: Option<&AtomicBool>,
) -> Result<ExitReason, HalError> {
```

3. Add the stop flag check at the top of the loop body, right after the iteration limit check (after the `if iterations > MAX_RUN_ITERATIONS` block):

```rust
        if let Some(flag) = stop_flag {
            if flag.load(Ordering::Relaxed) {
                return Ok(ExitReason::Canceled);
            }
        }
```

### Step 3: Thread stop flag through `boot_and_run`

In `crates/hitz-vmm/src/vm.rs`:

1. Add import:
```rust
use std::sync::atomic::AtomicBool;
```

2. Change `boot_and_run` signature:
```rust
pub fn boot_and_run<H: Hypervisor, W: Write>(
    hypervisor: &H,
    config: &VmConfig,
    serial_out: W,
    stop_flag: Option<&AtomicBool>,
) -> Result<VmRunResult, VmError> {
```

3. Update the `run_vcpu_loop` call (step 14) to pass the flag:
```rust
    // ── 14. Run vCPU loop ──
    let exit_reason =
        run_loop::run_vcpu_loop(&mut vcpu, &mut serial, &mut mmio_bus, &*guest_mem_arc, stop_flag)?;
```

### Step 4: Make `validate_config` public

In `crates/hitz-vmm/src/vm.rs`, change:
```rust
fn validate_config(config: &VmConfig) -> Result<(), VmError> {
```
To:
```rust
pub fn validate_config(config: &VmConfig) -> Result<(), VmError> {
```

### Step 5: Update re-exports in `lib.rs`

In `crates/hitz-vmm/src/lib.rs`, change the vm re-export line to:
```rust
pub use vm::{VmError, VmRunResult, boot_and_run, validate_config};
```

### Step 6: Update CLI call site

In `crates/hitz-cli/src/main.rs`, update the `boot_and_run` call:
```rust
    let result =
        hitz_vmm::boot_and_run(&hypervisor, &config, serial_out, None).context("VM boot failed")?;
```

Also update the match on exit reason to handle `Canceled`:
```rust
    match result.exit_reason {
        ExitReason::Halt | ExitReason::Canceled => Ok(ExitCode::SUCCESS),
        ExitReason::Shutdown | ExitReason::Unexpected(_) => Ok(ExitCode::FAILURE),
    }
```

### Step 7: Update WHP integration test call site

In `crates/hitz-whp/src/tests.rs`, line ~1331, update the `boot_and_run` call:
```rust
    let result = hitz_vmm::boot_and_run(&hv, &config, writer, None).expect("boot_and_run failed");
```

### Step 8: Run tests — verify all pass

Run: `cargo test --workspace`
Expected: all 121 tests pass (no new tests in this task — behavior tested via Task 6)

### Step 9: Commit

```bash
git add crates/hitz-vmm/src/run_loop.rs crates/hitz-vmm/src/vm.rs crates/hitz-vmm/src/lib.rs crates/hitz-cli/src/main.rs crates/hitz-whp/src/tests.rs
git commit -m "feat(vmm): add stop flag to boot_and_run + ExitReason::Canceled for graceful shutdown"
```

---

## Task 3: VmManager in `hitz-daemon`

**Files:**
- Modify: `crates/hitz-daemon/Cargo.toml`
- Modify: `crates/hitz-daemon/src/lib.rs`
- Create: `crates/hitz-daemon/src/error.rs`
- Create: `crates/hitz-daemon/src/vm_manager.rs`

### Step 1: Update `hitz-daemon/Cargo.toml`

Replace the full contents of `crates/hitz-daemon/Cargo.toml`:

```toml
[package]
name = "hitz-daemon"
description = "tokio runtime, named pipe server, VM coordinator"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
hitz-api.workspace = true
hitz-hal.workspace = true
hitz-vmm.workspace = true
thiserror.workspace = true
tracing.workspace = true
tokio.workspace = true
serde_json.workspace = true

[dev-dependencies]
tempfile = "3"
```

### Step 2: Create `error.rs`

Create `crates/hitz-daemon/src/error.rs`:

```rust
//! Daemon-specific error types.

use hitz_api::VmState;

/// Errors from daemon operations.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// VM with this ID was not found.
    #[error("VM not found: {0}")]
    NotFound(String),

    /// VM with this ID already exists.
    #[error("VM already exists: {0}")]
    AlreadyExists(String),

    /// VM is in a state that doesn't allow the requested action.
    #[error("VM \"{id}\" is {state:?}, expected {expected}")]
    InvalidState {
        /// VM identifier.
        id: String,
        /// Current state.
        state: VmState,
        /// What state was expected.
        expected: String,
    },

    /// VMM boot/run error.
    #[error("VMM error: {0}")]
    Vmm(#[from] hitz_vmm::VmError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error (lock poisoned, task panic, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}
```

### Step 3: Create `vm_manager.rs`

Create `crates/hitz-daemon/src/vm_manager.rs`:

```rust
//! VM lifecycle manager — coordinates `boot_and_run` tasks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hitz_api::{VmConfig, VmInfo, VmState};
use hitz_hal::Hypervisor;
use hitz_vmm::ExitReason;

use crate::error::DaemonError;

/// Internal state for a single VM.
struct VmEntry {
    config: VmConfig,
    state: VmState,
    exit_reason: Option<String>,
    stop_flag: Option<Arc<AtomicBool>>,
}

impl VmEntry {
    fn to_info(&self, id: &str) -> VmInfo {
        VmInfo {
            id: id.to_string(),
            state: self.state,
            config: self.config.clone(),
            exit_reason: self.exit_reason.clone(),
        }
    }
}

/// Manages the lifecycle of multiple VMs.
///
/// Generic over the hypervisor backend so the daemon library doesn't
/// depend on a specific platform (WHP, KVM, etc.).
#[derive(Clone)]
pub struct VmManager<H> {
    hypervisor: Arc<H>,
    vms: Arc<Mutex<HashMap<String, VmEntry>>>,
}

impl<H: Hypervisor + Send + Sync + 'static> VmManager<H> {
    /// Create a new manager with the given hypervisor backend.
    pub fn new(hypervisor: Arc<H>) -> Self {
        Self {
            hypervisor,
            vms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a VM with the given ID and config. Validates the config
    /// but does not boot.
    pub fn create_vm(&self, id: String, config: VmConfig) -> Result<VmInfo, DaemonError> {
        hitz_vmm::validate_config(&config)?;

        let mut vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;

        if vms.contains_key(&id) {
            return Err(DaemonError::AlreadyExists(id));
        }

        let entry = VmEntry {
            config,
            state: VmState::Created,
            exit_reason: None,
            stop_flag: None,
        };
        let info = entry.to_info(&id);
        vms.insert(id, entry);
        Ok(info)
    }

    /// Boot a previously created VM.
    pub fn start_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let (config, stop_flag, info) = {
            let mut vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;
            let entry = vms
                .get_mut(id)
                .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

            if entry.state != VmState::Created {
                return Err(DaemonError::InvalidState {
                    id: id.to_string(),
                    state: entry.state,
                    expected: "Created".to_string(),
                });
            }

            let flag = Arc::new(AtomicBool::new(false));
            entry.stop_flag = Some(flag.clone());
            entry.state = VmState::Running;

            (entry.config.clone(), flag, entry.to_info(id))
        };
        // Mutex released here — spawn_blocking must not hold it.

        let hv = self.hypervisor.clone();
        let vms = self.vms.clone();
        let vm_id = id.to_string();

        tokio::task::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                // Serial output goes to a sink (log file deferred).
                hitz_vmm::boot_and_run(&*hv, &config, std::io::sink(), Some(&*stop_flag))
            })
            .await;

            // Update state based on result.
            if let Ok(mut vms) = vms.lock() {
                if let Some(entry) = vms.get_mut(&vm_id) {
                    match result {
                        Ok(Ok(run_result)) => match run_result.exit_reason {
                            ExitReason::Halt | ExitReason::Canceled => {
                                entry.state = VmState::Stopped;
                                entry.exit_reason =
                                    Some(format!("{:?}", run_result.exit_reason));
                            }
                            reason => {
                                entry.state = VmState::Failed;
                                entry.exit_reason = Some(format!("{reason:?}"));
                            }
                        },
                        Ok(Err(e)) => {
                            entry.state = VmState::Failed;
                            entry.exit_reason = Some(e.to_string());
                        }
                        Err(e) => {
                            entry.state = VmState::Failed;
                            entry.exit_reason = Some(format!("task panicked: {e}"));
                        }
                    }
                    entry.stop_flag = None;
                }
            }
        });

        Ok(info)
    }

    /// Stop a running VM by setting its stop flag.
    pub fn stop_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let mut vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get_mut(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        if entry.state != VmState::Running {
            return Err(DaemonError::InvalidState {
                id: id.to_string(),
                state: entry.state,
                expected: "Running".to_string(),
            });
        }

        if let Some(ref flag) = entry.stop_flag {
            flag.store(true, Ordering::Relaxed);
        }

        Ok(entry.to_info(id))
    }

    /// Get info about a single VM.
    pub fn get_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;
        Ok(entry.to_info(id))
    }

    /// List all VMs.
    pub fn list_vms(&self) -> Result<Vec<VmInfo>, DaemonError> {
        let vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;
        Ok(vms.iter().map(|(id, entry)| entry.to_info(id)).collect())
    }

    /// Delete a VM. Must not be running.
    pub fn delete_vm(&self, id: &str) -> Result<(), DaemonError> {
        let mut vms = self.vms.lock().map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        if entry.state == VmState::Running {
            return Err(DaemonError::InvalidState {
                id: id.to_string(),
                state: entry.state,
                expected: "Created, Stopped, or Failed".to_string(),
            });
        }

        vms.remove(id);
        Ok(())
    }

    /// Stop all running VMs. Used for daemon shutdown.
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
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use hitz_api::VmConfig;
    use std::path::PathBuf;

    /// Dummy hypervisor for testing state transitions without WHP.
    struct FakeHypervisor;

    impl Hypervisor for FakeHypervisor {
        type P = FakePartition;
        fn create_partition(
            &self,
            _config: &hitz_hal::PartitionConfig,
        ) -> Result<Self::P, hitz_hal::HalError> {
            // boot_and_run will fail at partition creation,
            // but we can still test create/get/list/delete.
            Err(hitz_hal::HalError::Other("fake hypervisor".into()))
        }
    }

    struct FakePartition;

    impl hitz_hal::Partition for FakePartition {
        type V = FakeVcpu;
        fn map_memory(
            &mut self,
            _gpa: hitz_hal::Gpa,
            _hva: *const u8,
            _size: u64,
            _flags: hitz_hal::MemFlags,
        ) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn unmap_memory(&mut self, _gpa: hitz_hal::Gpa, _size: u64) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn create_vcpu(&mut self, _id: hitz_hal::VcpuId) -> Result<Self::V, hitz_hal::HalError> {
            Ok(FakeVcpu)
        }
        fn request_interrupt(
            &self,
            _vcpu_id: hitz_hal::VcpuId,
            _vector: u8,
        ) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
    }

    struct FakeVcpu;

    impl hitz_hal::Vcpu for FakeVcpu {
        fn run(&mut self) -> Result<hitz_hal::VcpuExit, hitz_hal::HalError> {
            Ok(hitz_hal::VcpuExit::Halt)
        }
        fn cancel(&self) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn get_regs(&self) -> Result<hitz_hal::StandardRegs, hitz_hal::HalError> {
            Ok(hitz_hal::StandardRegs::default())
        }
        fn set_regs(&mut self, _regs: &hitz_hal::StandardRegs) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, hitz_hal::HalError> {
            Ok(hitz_hal::SpecialRegs::default())
        }
        fn set_sregs(&mut self, _sregs: &hitz_hal::SpecialRegs) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
    }

    fn make_manager() -> VmManager<FakeHypervisor> {
        VmManager::new(Arc::new(FakeHypervisor))
    }

    fn make_config() -> VmConfig {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        // Leak the temp file so it survives the test.
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        VmConfig {
            kernel_path: path,
            initramfs_path: None,
            disk_path: None,
            ram_mib: 128,
            cmdline: None,
        }
    }

    #[test]
    fn create_vm_sets_created_state() {
        let mgr = make_manager();
        let info = mgr.create_vm("vm1".into(), make_config()).expect("create");
        assert_eq!(info.state, VmState::Created);
        assert_eq!(info.id, "vm1");
    }

    #[test]
    fn create_duplicate_vm_fails() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        let err = mgr.create_vm("vm1".into(), make_config()).unwrap_err();
        assert!(err.to_string().contains("already exists"), "got: {err}");
    }

    #[test]
    fn get_vm_not_found() {
        let mgr = make_manager();
        let err = mgr.get_vm("nope").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn list_vms_empty() {
        let mgr = make_manager();
        let list = mgr.list_vms().expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn list_vms_after_create() {
        let mgr = make_manager();
        mgr.create_vm("a".into(), make_config()).expect("create a");
        mgr.create_vm("b".into(), make_config()).expect("create b");
        let list = mgr.list_vms().expect("list");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_created_vm() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        mgr.delete_vm("vm1").expect("delete");
        let err = mgr.get_vm("vm1").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_nonexistent_vm_fails() {
        let mgr = make_manager();
        let err = mgr.delete_vm("nope").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn stop_created_vm_fails() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        let err = mgr.stop_vm("vm1").unwrap_err();
        assert!(err.to_string().contains("Created"), "got: {err}");
    }
}
```

### Step 4: Update `lib.rs`

Replace `crates/hitz-daemon/src/lib.rs` with:

```rust
//! Hitz daemon — named pipe HTTP server and VM coordinator.

pub mod error;
pub mod vm_manager;

pub use error::DaemonError;
pub use vm_manager::VmManager;
```

### Step 5: Run tests — verify they pass

Run: `cargo test -p hitz-daemon`
Expected: 8 tests pass

Run: `cargo test --workspace`
Expected: all tests pass (~129 total: 121 existing + 8 new)

### Step 6: Commit

```bash
git add crates/hitz-daemon/
git commit -m "feat(daemon): VmManager with create/start/stop/delete/list and state machine"
```

---

## Task 4: Named Pipe Server + HTTP Router

**Files:**
- Modify: `Cargo.toml` (workspace — add `bytes`)
- Modify: `crates/hitz-daemon/Cargo.toml` (add hyper, bytes, http-body-util, hyper-util)
- Create: `crates/hitz-daemon/src/router.rs`
- Create: `crates/hitz-daemon/src/server.rs`
- Modify: `crates/hitz-daemon/src/lib.rs`

### Step 1: Add `bytes` to workspace dependencies

In `Cargo.toml` (workspace root), add to `[workspace.dependencies]`:
```toml
bytes = "1"
```

### Step 2: Update `hitz-daemon/Cargo.toml`

Add the server dependencies:

```toml
[package]
name = "hitz-daemon"
description = "tokio runtime, named pipe server, VM coordinator"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
hitz-api.workspace = true
hitz-hal.workspace = true
hitz-vmm.workspace = true
thiserror.workspace = true
tracing.workspace = true
tokio.workspace = true
serde_json.workspace = true
hyper.workspace = true
http-body-util.workspace = true
hyper-util.workspace = true
bytes.workspace = true

[dev-dependencies]
tempfile = "3"
```

### Step 3: Create `router.rs`

Create `crates/hitz-daemon/src/router.rs`:

```rust
//! HTTP request router — dispatches to `VmManager` methods.

use std::convert::Infallible;

use bytes::Bytes;
use hitz_api::{ActionVmRequest, ApiError, CreateVmRequest, VmAction};
use hitz_hal::Hypervisor;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response, StatusCode, body::Incoming};

use crate::error::DaemonError;
use crate::vm_manager::VmManager;

/// Route an incoming HTTP request to the appropriate handler.
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let result = match (method, path.as_str()) {
        (Method::GET, "/vms") => handle_list(manager),
        _ if path.starts_with("/vms/") => {
            let segments: Vec<&str> = path.splitn(4, '/').collect();
            // segments: ["", "vms", "{id}", "action"?]
            match segments.get(2) {
                Some(id) if !id.is_empty() => {
                    let suffix = segments.get(3).copied();
                    route_vm(req, &method, id, suffix, manager).await
                }
                _ => Ok(error_response(StatusCode::BAD_REQUEST, "missing VM ID")),
            }
        }
        _ => Ok(error_response(StatusCode::NOT_FOUND, "not found")),
    };

    Ok(result.unwrap_or_else(|e| daemon_error_response(&e)))
}

async fn route_vm<H>(
    req: Request<Incoming>,
    method: &Method,
    id: &str,
    suffix: Option<&str>,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    match (method, suffix) {
        (&Method::PUT, None) => handle_create(req, id, manager).await,
        (&Method::GET, None) => handle_get(id, manager),
        (&Method::DELETE, None) => handle_delete(id, manager),
        (&Method::POST, Some("action")) => handle_action(req, id, manager).await,
        _ => Ok(error_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed")),
    }
}

async fn handle_create<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body = req.into_body().collect().await.map_err(|e| {
        DaemonError::Internal(format!("failed to read request body: {e}"))
    })?;
    let create_req: CreateVmRequest = serde_json::from_slice(&body.to_bytes()).map_err(|e| {
        DaemonError::Internal(format!("invalid JSON: {e}"))
    })?;

    let info = manager.create_vm(id.to_string(), create_req.config)?;
    json_response(StatusCode::CREATED, &info)
}

fn handle_get<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let info = manager.get_vm(id)?;
    json_response(StatusCode::OK, &info)
}

fn handle_list<H>(
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let list = manager.list_vms()?;
    json_response(StatusCode::OK, &list)
}

fn handle_delete<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    manager.delete_vm(id)?;
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Full::new(Bytes::new()))
        .expect("build empty response"))
}

async fn handle_action<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body = req.into_body().collect().await.map_err(|e| {
        DaemonError::Internal(format!("failed to read request body: {e}"))
    })?;
    let action_req: ActionVmRequest = serde_json::from_slice(&body.to_bytes()).map_err(|e| {
        DaemonError::Internal(format!("invalid JSON: {e}"))
    })?;

    let info = match action_req.action {
        VmAction::Start => manager.start_vm(id)?,
        VmAction::Stop => manager.stop_vm(id)?,
    };
    json_response(StatusCode::OK, &info)
}

fn json_response<T: serde::Serialize>(
    status: StatusCode,
    body: &T,
) -> Result<Response<Full<Bytes>>, DaemonError> {
    let json = serde_json::to_string(body)
        .map_err(|e| DaemonError::Internal(format!("JSON serialize: {e}")))?;
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .expect("build json response"))
}

fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    let body = ApiError {
        message: message.to_string(),
    };
    let json = serde_json::to_string(&body).unwrap_or_else(|_| r#"{"message":"internal error"}"#.to_string());
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .expect("build error response")
}

fn daemon_error_response(err: &DaemonError) -> Response<Full<Bytes>> {
    let status = match err {
        DaemonError::NotFound(_) => StatusCode::NOT_FOUND,
        DaemonError::AlreadyExists(_) | DaemonError::InvalidState { .. } => StatusCode::CONFLICT,
        DaemonError::Vmm(hitz_vmm::VmError::Config(_)) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error_response(status, &err.to_string())
}
```

### Step 4: Create `server.rs`

Create `crates/hitz-daemon/src/server.rs`:

```rust
//! Named pipe HTTP server.
//!
//! Listens on a Windows named pipe and serves HTTP/1.1 requests via hyper.
//! Each client connection gets its own tokio task.

use hitz_hal::Hypervisor;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::windows::named_pipe::ServerOptions;

use crate::router;
use crate::vm_manager::VmManager;

/// Run the named pipe HTTP server.
///
/// Blocks until the future is canceled (typically by `tokio::select!` with
/// Ctrl+C). Each client connection is handled in a spawned task.
pub async fn run_server<H>(
    pipe_path: &str,
    manager: VmManager<H>,
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

        // Wait for a client to connect.
        server.connect().await?;
        tracing::debug!("client connected to {pipe_path}");

        let mgr = manager.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(server);
            let service = service_fn(move |req| {
                let m = mgr.clone();
                async move { router::route(req, &m).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::error!("connection error: {e}");
            }
        });
    }
}
```

### Step 5: Update `lib.rs`

Replace `crates/hitz-daemon/src/lib.rs`:

```rust
//! Hitz daemon — named pipe HTTP server and VM coordinator.

pub mod error;
pub mod router;
pub mod server;
pub mod vm_manager;

pub use error::DaemonError;
pub use server::run_server;
pub use vm_manager::VmManager;
```

### Step 6: Verify it compiles

Run: `cargo build -p hitz-daemon`
Expected: compiles without errors

Run: `cargo clippy -p hitz-daemon -- -D warnings`
Expected: no warnings

Run: `cargo test --workspace`
Expected: all tests pass

### Step 7: Commit

```bash
git add Cargo.toml Cargo.lock crates/hitz-daemon/
git commit -m "feat(daemon): named pipe HTTP server with hyper router"
```

---

## Task 5: CLI Subcommands + Ctrl+C

**Files:**
- Modify: `Cargo.toml` (workspace — add `ctrlc`)
- Modify: `crates/hitz-cli/Cargo.toml`
- Create: `crates/hitz-cli/src/pipe_client.rs`
- Modify: `crates/hitz-cli/src/main.rs`

### Step 1: Add `ctrlc` to workspace

In `Cargo.toml` (workspace root), add to `[workspace.dependencies]`:
```toml
ctrlc = "3"
```

### Step 2: Update `hitz-cli/Cargo.toml`

Replace the full contents:

```toml
[package]
name = "hitz-cli"
description = "CLI binary — clap + named pipe client"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "hitz"
path = "src/main.rs"

[lints]
workspace = true

[dependencies]
hitz-api.workspace = true
hitz-hal.workspace = true
hitz-vmm.workspace = true
hitz-whp.workspace = true
hitz-daemon.workspace = true
anyhow.workspace = true
clap.workspace = true
tracing-subscriber.workspace = true
tokio.workspace = true
hyper = { workspace = true, features = ["client"] }
http-body-util.workspace = true
hyper-util.workspace = true
serde_json.workspace = true
bytes.workspace = true
ctrlc.workspace = true
```

### Step 3: Create `pipe_client.rs`

Create `crates/hitz-cli/src/pipe_client.rs`:

```rust
//! Named pipe HTTP client for talking to the hitz daemon.

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::windows::named_pipe::ClientOptions;

/// Send an HTTP request to the daemon over the named pipe.
///
/// Returns `(status_code, response_body_string)`.
pub async fn pipe_request(
    pipe_path: &str,
    method: Method,
    path: &str,
    body: Option<&str>,
) -> Result<(StatusCode, String)> {
    let pipe = ClientOptions::new()
        .open(pipe_path)
        .with_context(|| format!("cannot connect to daemon at {pipe_path} — is it running?"))?;

    let io = TokioIo::new(pipe);
    let (mut sender, conn) = http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;

    tokio::spawn(conn);

    let req_body = match body {
        Some(b) => Full::new(Bytes::from(b.to_string())),
        None => Full::new(Bytes::new()),
    };

    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(req_body)
        .context("build request")?;

    let resp = sender
        .send_request(req)
        .await
        .context("send request failed")?;

    let status = resp.status();
    let resp_body = resp
        .into_body()
        .collect()
        .await
        .context("read response body")?
        .to_bytes();
    let text = String::from_utf8_lossy(&resp_body).to_string();

    Ok((status, text))
}
```

### Step 4: Rewrite `main.rs`

Replace `crates/hitz-cli/src/main.rs` with:

```rust
//! Hitz CLI — micro-VM manager for Windows.
//!
//! Usage:
//!   `hitz run --kernel vmlinux [--initramfs init.cpio] [-v]`
//!   `hitz daemon start [--pipe \\.\pipe\hitz]`
//!   `hitz vm create <ID> --kernel vmlinux [--initramfs ...]`
//!   `hitz vm start <ID>`

// CLI binary — anyhow for top-level errors, expect on infallible ops.
#![allow(clippy::expect_used)]

mod pipe_client;

use std::io::{BufWriter, Write, stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hitz_api::{
    ActionVmRequest, CreateVmRequest, DEFAULT_CMDLINE, DEFAULT_RAM_MIB, VmAction, VmConfig,
};
use hitz_vmm::ExitReason;
use hitz_whp::WhpHypervisor;
use hyper::Method;

/// Default named pipe path for the daemon.
const DEFAULT_PIPE: &str = r"\\.\pipe\hitz";

/// Hitz — Hyper-V micro-VM manager for Windows.
#[derive(Parser)]
#[command(name = "hitz", version, about)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Available subcommands.
#[derive(Subcommand)]
enum Command {
    /// Boot a Linux kernel in a micro-VM (standalone, no daemon).
    Run(RunArgs),
    /// Daemon management.
    #[command(subcommand)]
    Daemon(DaemonCommand),
    /// VM lifecycle (requires running daemon).
    #[command(subcommand)]
    Vm(VmCommand),
}

// ── Run (standalone) ──

/// Arguments for the `run` subcommand.
#[derive(Parser)]
struct RunArgs {
    /// Path to the kernel ELF binary (vmlinux).
    #[arg(long)]
    kernel: PathBuf,

    /// Path to an initramfs (cpio archive).
    #[arg(long)]
    initramfs: Option<PathBuf>,

    /// Path to a disk image for virtio-blk.
    #[arg(long)]
    disk: Option<PathBuf>,

    /// Guest RAM in MiB.
    #[arg(long, default_value_t = DEFAULT_RAM_MIB)]
    ram: u32,

    /// Kernel command line.
    #[arg(long, default_value = DEFAULT_CMDLINE)]
    cmdline: String,

    /// Verbose output (banner + exit summary on stderr).
    #[arg(short, long)]
    verbose: bool,
}

// ── Daemon ──

/// Daemon management subcommands.
#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the daemon in the foreground.
    Start(DaemonStartArgs),
}

/// Arguments for `daemon start`.
#[derive(Parser)]
struct DaemonStartArgs {
    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Verbose output.
    #[arg(short, long)]
    verbose: bool,
}

// ── VM ──

/// VM lifecycle subcommands (requires running daemon).
#[derive(Subcommand)]
enum VmCommand {
    /// Create a VM with the given configuration.
    Create(VmCreateArgs),
    /// Start (boot) a previously created VM.
    Start(VmIdArgs),
    /// Stop a running VM.
    Stop(VmIdArgs),
    /// Get VM status.
    Status(VmIdArgs),
    /// List all VMs.
    List(VmListArgs),
    /// Delete a stopped VM.
    Delete(VmIdArgs),
}

/// Arguments for `vm create`.
#[derive(Parser)]
struct VmCreateArgs {
    /// VM identifier.
    id: String,

    /// Path to the kernel ELF binary (vmlinux).
    #[arg(long)]
    kernel: PathBuf,

    /// Path to an initramfs (cpio archive).
    #[arg(long)]
    initramfs: Option<PathBuf>,

    /// Path to a disk image for virtio-blk.
    #[arg(long)]
    disk: Option<PathBuf>,

    /// Guest RAM in MiB.
    #[arg(long, default_value_t = DEFAULT_RAM_MIB)]
    ram: u32,

    /// Kernel command line.
    #[arg(long, default_value = DEFAULT_CMDLINE)]
    cmdline: String,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,
}

/// Arguments that take just a VM ID.
#[derive(Parser)]
struct VmIdArgs {
    /// VM identifier.
    id: String,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,
}

/// Arguments for `vm list`.
#[derive(Parser)]
struct VmListArgs {
    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,
}

// ── Main ──

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Run(args) => match run_vm(args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        },
        Command::Daemon(cmd) => match cmd {
            DaemonCommand::Start(args) => match run_daemon(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::FAILURE
                }
            },
        },
        Command::Vm(cmd) => match run_vm_command(cmd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        },
    }
}

// ── hitz run ──

/// Execute the `run` subcommand (standalone, no daemon).
fn run_vm(args: RunArgs) -> Result<ExitCode> {
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("hitz=debug")
            .with_writer(std::io::stderr)
            .init();

        eprintln!("hitz: booting kernel {}", args.kernel.display());
        if let Some(ref initramfs) = args.initramfs {
            eprintln!("hitz: initramfs {}", initramfs.display());
        }
        if let Some(ref disk) = args.disk {
            eprintln!("hitz: disk {}", disk.display());
        }
        eprintln!("hitz: RAM {} MiB", args.ram);
        eprintln!("hitz: cmdline \"{}\"", args.cmdline);
    }

    let config = VmConfig {
        kernel_path: args.kernel,
        initramfs_path: args.initramfs,
        disk_path: args.disk,
        ram_mib: args.ram,
        cmdline: Some(args.cmdline),
    };

    let hypervisor = WhpHypervisor::new().context("failed to create WHP hypervisor")?;

    // Set up Ctrl+C handler to stop the VM gracefully.
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::Relaxed);
    })
    .context("failed to set Ctrl+C handler")?;

    // BufWriter avoids per-byte lock on stdout (serial writes one byte at a time).
    let serial_out = BufWriter::new(stdout().lock());

    let result = hitz_vmm::boot_and_run(&hypervisor, &config, serial_out, Some(&stop_flag))
        .context("VM boot failed")?;

    // Flush stdout before printing to stderr.
    let _ = stdout().flush();

    if args.verbose {
        eprintln!("hitz: VM exited: {:?}", result.exit_reason);
    }

    match result.exit_reason {
        ExitReason::Halt | ExitReason::Canceled => Ok(ExitCode::SUCCESS),
        ExitReason::Shutdown | ExitReason::Unexpected(_) => Ok(ExitCode::FAILURE),
    }
}

// ── hitz daemon start ──

/// Run the daemon in the foreground.
fn run_daemon(args: DaemonStartArgs) -> Result<()> {
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("hitz=debug")
            .with_writer(std::io::stderr)
            .init();
    }

    eprintln!("hitz: daemon listening on {}", args.pipe);

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().context("WHP not available")?);
        let manager = hitz_daemon::VmManager::new(hv);

        tokio::select! {
            result = hitz_daemon::run_server(&args.pipe, manager.clone()) => {
                result.context("server error")?;
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nhitz: shutting down...");
                manager.stop_all();
                // Brief wait for VM tasks to finish.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        Ok(())
    })
}

// ── hitz vm * ──

/// Execute a `vm` subcommand by talking to the daemon over the named pipe.
fn run_vm_command(cmd: VmCommand) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    rt.block_on(async {
        match cmd {
            VmCommand::Create(args) => {
                let config = VmConfig {
                    kernel_path: args.kernel,
                    initramfs_path: args.initramfs,
                    disk_path: args.disk,
                    ram_mib: args.ram,
                    cmdline: Some(args.cmdline),
                };
                let body = serde_json::to_string(&CreateVmRequest { config })
                    .context("serialize request")?;
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    Method::PUT,
                    &format!("/vms/{}", args.id),
                    Some(&body),
                )
                .await?;
                println!("{status}: {resp}");
            }
            VmCommand::Start(args) => {
                let body = serde_json::to_string(&ActionVmRequest {
                    action: VmAction::Start,
                })
                .context("serialize request")?;
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    Method::POST,
                    &format!("/vms/{}/action", args.id),
                    Some(&body),
                )
                .await?;
                println!("{status}: {resp}");
            }
            VmCommand::Stop(args) => {
                let body = serde_json::to_string(&ActionVmRequest {
                    action: VmAction::Stop,
                })
                .context("serialize request")?;
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    Method::POST,
                    &format!("/vms/{}/action", args.id),
                    Some(&body),
                )
                .await?;
                println!("{status}: {resp}");
            }
            VmCommand::Status(args) => {
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    Method::GET,
                    &format!("/vms/{}", args.id),
                    None,
                )
                .await?;
                println!("{status}: {resp}");
            }
            VmCommand::List(args) => {
                let (status, resp) =
                    pipe_client::pipe_request(&args.pipe, Method::GET, "/vms", None).await?;
                println!("{status}: {resp}");
            }
            VmCommand::Delete(args) => {
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    Method::DELETE,
                    &format!("/vms/{}", args.id),
                    None,
                )
                .await?;
                if status.is_success() {
                    println!("deleted {}", args.id);
                } else {
                    println!("{status}: {resp}");
                }
            }
        }
        Ok(())
    })
}
```

### Step 5: Verify build and CLI help

Run: `cargo build --bin hitz`
Expected: compiles

Run: `cargo run --bin hitz -- --help`
Expected: shows `run`, `daemon`, `vm` subcommands

Run: `cargo run --bin hitz -- daemon start --help`
Expected: shows `--pipe` and `-v` options

Run: `cargo run --bin hitz -- vm --help`
Expected: shows `create`, `start`, `stop`, `status`, `list`, `delete`

### Step 6: Run full test suite

Run: `cargo fmt`
Run: `cargo clippy --workspace -- -D warnings`
Run: `cargo test --workspace`
Expected: all tests pass

### Step 7: Commit

```bash
git add Cargo.toml Cargo.lock crates/hitz-cli/ crates/hitz-daemon/
git commit -m "feat(cli): daemon start, vm subcommands, Ctrl+C graceful shutdown, named pipe client"
```

---

## Task 6: Integration Test

**Files:**
- Modify: `crates/hitz-whp/src/tests.rs`
- Modify: `crates/hitz-whp/Cargo.toml`

### Step 1: Update `hitz-whp/Cargo.toml`

Add `hitz-daemon` and `tokio` to dev-dependencies:

```toml
[dev-dependencies]
anyhow.workspace = true
hitz-api.workspace = true
hitz-boot.workspace = true
hitz-devices.workspace = true
hitz-vmm.workspace = true
hitz-daemon.workspace = true
tokio.workspace = true
tempfile = "3"
windows = { workspace = true, features = [
    "Win32_System_Memory",
] }
```

### Step 2: Add Phase 6 test

Append to `crates/hitz-whp/src/tests.rs`, after the Phase 5 section:

```rust
// ─── Phase 6: daemon VmManager lifecycle ─────────────────────────────────────

/// Phase 6 checkpoint: VmManager can create, start, stop, and delete a VM
/// using the real WHP hypervisor.
///
/// Creates a "Hello" ELF VM through the daemon's VmManager, starts it,
/// waits for it to exit (the HLT halts the vCPU loop), and verifies the
/// state transitions: Created → Running → Stopped. Then deletes the VM.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase6_vm_manager_lifecycle() {
    use std::io::Write;
    use std::sync::Arc;

    use hitz_api::{VmConfig, VmState};
    use hitz_daemon::VmManager;

    // x86-64 machine code: writes "Hello" to COM1 (0x3F8) then halts.
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
        cmdline: Some("console=ttyS0\0".into()),
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().expect("WHP not available"));
        let manager = VmManager::new(hv);

        // Create
        let info = manager
            .create_vm("test-vm".into(), config)
            .expect("create_vm");
        assert_eq!(info.state, VmState::Created);

        // Start
        let info = manager.start_vm("test-vm").expect("start_vm");
        assert_eq!(info.state, VmState::Running);

        // Wait for the VM to finish (the "Hello" program halts quickly).
        // Poll status until it changes from Running.
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let info = manager.get_vm("test-vm").expect("get_vm");
            if info.state != VmState::Running {
                assert_eq!(
                    info.state,
                    VmState::Stopped,
                    "expected Stopped, got {:?} (exit: {:?})",
                    info.state,
                    info.exit_reason
                );
                break;
            }
        }

        let info = manager.get_vm("test-vm").expect("get_vm final");
        assert_eq!(info.state, VmState::Stopped, "VM should have stopped");

        // Delete
        manager.delete_vm("test-vm").expect("delete_vm");
        let err = manager.get_vm("test-vm").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    });
}
```

### Step 3: Run workspace tests

Run: `cargo test --workspace`
Expected: all non-ignored tests pass

Run: `cargo fmt`
Run: `cargo clippy --workspace -- -D warnings`
Expected: clean

### Step 4: Commit

```bash
git add crates/hitz-whp/Cargo.toml crates/hitz-whp/src/tests.rs Cargo.lock
git commit -m "feat: Phase 6 — daemon, named pipe server, multi-VM management, Ctrl+C shutdown"
```

---

## Verification Checklist

After all tasks:

1. `cargo fmt --check` — clean
2. `cargo clippy --workspace -- -D warnings` — clean
3. `cargo test --workspace` — all tests pass (~137: 121 existing + 6 API + 8 VmManager + 2 other)
4. `cargo test -p hitz-whp -- --ignored --test-threads=1` — Phase 6 lifecycle test
5. `cargo run --bin hitz -- --help` — shows `run`, `daemon`, `vm`
6. `cargo run --bin hitz -- run --help` — shows `--kernel` etc.
7. `cargo run --bin hitz -- daemon start --help` — shows `--pipe`, `-v`
8. `cargo run --bin hitz -- vm create --help` — shows `<ID>`, `--kernel`, etc.
9. Manual test (two terminals):
   - Terminal 1: `cargo run --bin hitz -- daemon start -v`
   - Terminal 2: `cargo run --bin hitz -- vm list`
   - Terminal 2: `cargo run --bin hitz -- vm create test1 --kernel vmlinux`
   - Terminal 2: `cargo run --bin hitz -- vm status test1`
   - Terminal 2: `cargo run --bin hitz -- vm delete test1`
   - Terminal 1: Ctrl+C
