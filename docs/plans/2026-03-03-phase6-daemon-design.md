# Phase 6: Daemon, Named Pipe Server, Multi-VM Management

## Goal

Background daemon process listening on `\\.\pipe\hitz`, exposing a REST-style JSON API over HTTP/1.1 for multi-VM lifecycle management. CLI gains `hitz daemon start` and `hitz vm create/start/stop/status/list/delete` subcommands. `hitz run` gains Ctrl+C graceful shutdown.

## Architecture

```
+--------------------------------------------------+
|  hitz daemon start  (foreground, tokio runtime)   |
|                                                   |
|  +-----------+    +------------------------+      |
|  | Named Pipe|---→|  HTTP Router (hyper)   |      |
|  | Listener  |    |  PUT/POST/GET/DELETE   |      |
|  +-----------+    +-----------+------------+      |
|                               |                   |
|                    +----------v-----------+       |
|                    |    VmManager          |       |
|                    |  HashMap<String, Vm>  |       |
|                    +----------+-----------+       |
|                               |                   |
|              +----------------+----------+        |
|              v                v          v        |
|         +---------+    +---------+  +---------+   |
|         |  VM #1  |    |  VM #2  |  |  VM #N  |   |
|         | (spawn_ |    | (spawn_ |  | (spawn_ |   |
|         |blocking)|    |blocking)|  |blocking)|   |
|         +---------+    +---------+  +---------+   |
+--------------------------------------------------+

+--------------------+
|  hitz vm create .. |---- named pipe ----→ daemon
|  hitz vm start ..  |
|  hitz vm stop ..   |
+--------------------+

+--------------------+
|  hitz run --kernel |  (standalone, no daemon)
|  (+ Ctrl+C handler)|
+--------------------+
```

## Design Decisions

- **Named pipe first** — secure default (no network exposure), TCP opt-in later.
- **HTTP/1.1 over pipe** — reuse hyper routing/parsing, same handlers work for future TCP.
- **Foreground daemon** — `hitz daemon start` runs in foreground, logs to stderr. Pipe existence = health check.
- **Separate create/start** — validate config before committing resources (Firecracker pattern).
- **Serial to log file** — live streaming deferred to later phase.
- **`hitz run` stays standalone** — one-shot headless boot, no daemon required. Scriptable like `docker run --rm`.
- **`AtomicBool` stop flag** — threaded into `boot_and_run` → `run_vcpu_loop`, checked each iteration. Minimal API change.
- **Simple 5-state machine** — Created, Running, Stopped, Failed. No intermediate Booting/Stopping states.

## API Surface

| Method | Endpoint | Body | Response |
|--------|----------|------|----------|
| `PUT` | `/vms/{id}` | `CreateVmRequest` | `VmInfo` (201) |
| `POST` | `/vms/{id}/action` | `ActionVmRequest` | `VmInfo` (200) |
| `GET` | `/vms/{id}` | — | `VmInfo` (200) |
| `GET` | `/vms` | — | `Vec<VmInfo>` (200) |
| `DELETE` | `/vms/{id}` | — | 204 |

## API Types (`hitz-api`)

```rust
pub enum VmAction { Start, Stop }

pub struct CreateVmRequest { pub config: VmConfig }
pub struct ActionVmRequest { pub action: VmAction }

pub enum VmState { Created, Running, Stopped, Failed }

pub struct VmInfo {
    pub id: String,
    pub state: VmState,
    pub config: VmConfig,
    pub exit_reason: Option<String>,
}

pub struct ApiError { pub message: String }
```

## VM Manager (`hitz-daemon`)

```rust
struct VmEntry {
    config: VmConfig,
    state: VmState,
    exit_reason: Option<String>,
    cancel_tx: Option<oneshot::Sender<()>>,
    serial_log: PathBuf,
}

pub struct VmManager {
    vms: Mutex<HashMap<String, VmEntry>>,
}
```

### Lifecycle Flows

**Create**: validate config → insert entry as `Created` → return `VmInfo`.

**Start**: check state is `Created` → set `Running` → open serial log → `spawn_blocking(boot_and_run)` → on completion update to `Stopped` or `Failed`.

**Stop**: check state is `Running` → set stop flag via `cancel_tx` → `run_vcpu_loop` sees flag, calls `vcpu.cancel()` → loop exits → task completes → state updated.

**Delete**: check state is not `Running` → remove entry.

## Named Pipe Transport

```rust
// Server: create pipe instances in a loop
loop {
    let server = ServerOptions::new()
        .first_pipe_instance(first)
        .create(r"\\.\pipe\hitz")?;
    server.connect().await?;
    first = false;
    let io = TokioIo::new(server);
    tokio::spawn(async move {
        http1::Builder::new()
            .serve_connection(io, service_fn(|req| handle(req, &manager)))
            .await
    });
}

// Client: connect and send HTTP request
let pipe = ClientOptions::new().open(r"\\.\pipe\hitz")?;
let io = TokioIo::new(pipe);
let (mut sender, conn) = http1::handshake(io).await?;
tokio::spawn(conn);
let resp = sender.send_request(req).await?;
```

## CLI Subcommand Tree

```
hitz
├── run          (standalone + Ctrl+C handler)
├── daemon
│   └── start    (foreground daemon, --pipe option)
└── vm
    ├── create   PUT /vms/{id}
    ├── start    POST /vms/{id}/action
    ├── stop     POST /vms/{id}/action
    ├── status   GET /vms/{id}
    ├── list     GET /vms
    └── delete   DELETE /vms/{id}
```

## Stop Flag Mechanism

```rust
pub fn boot_and_run<H: Hypervisor, W: Write>(
    hypervisor: &H,
    config: &VmConfig,
    serial_out: W,
    stop_flag: Option<&AtomicBool>,  // NEW
) -> Result<VmRunResult, VmError>

// In run_vcpu_loop, top of each iteration:
if let Some(flag) = stop_flag {
    if flag.load(Ordering::Relaxed) {
        return Ok(ExitReason::Shutdown);
    }
}
```

## Task Breakdown

### Task 1: API types in `hitz-api` (independent)
- `VmAction`, `VmState`, `CreateVmRequest`, `ActionVmRequest`, `VmInfo`, `ApiError`
- Serde derives, tests

### Task 2: Stop flag in `boot_and_run` (independent)
- Add `stop_flag: Option<&AtomicBool>` to `boot_and_run` and `run_vcpu_loop`
- Update CLI and integration test call sites to pass `None`

### Task 3: VmManager (depends on 1, 2)
- `hitz-daemon/src/vm_manager.rs`
- State machine, `spawn_blocking`, cancellation via `AtomicBool`
- Unit tests for state transitions

### Task 4: Named pipe server + HTTP router (depends on 3)
- `hitz-daemon/src/server.rs` — pipe listener loop
- `hitz-daemon/src/router.rs` — route dispatch, JSON handling
- `hitz-daemon/src/main.rs` — tokio runtime, Ctrl+C for daemon

### Task 5: CLI subcommands (depends on 4)
- `hitz-cli/src/pipe_client.rs` — named pipe HTTP client
- `hitz daemon start`, `hitz vm *` subcommands
- `hitz run` Ctrl+C handler with stop flag
- Add `ctrlc` crate

### Task 6: Integration test (depends on 3)
- In-process VmManager test: create → start → verify Running → stop → verify Stopped

## Dependency Graph

```
Task 1 (API types) --+
                     +--→ Task 3 (VmManager) --→ Task 4 (server) --→ Task 5 (CLI)
Task 2 (stop flag) --+                    |
                                          +--→ Task 6 (integration test)
```

## Files

| File | Action | Task |
|------|--------|------|
| `hitz-api/src/lib.rs` | Modify | 1 |
| `hitz-vmm/src/vm.rs` | Modify | 2 |
| `hitz-vmm/src/run_loop.rs` | Modify | 2 |
| `hitz-vmm/src/lib.rs` | Modify | 2 |
| `hitz-cli/src/main.rs` | Modify | 2, 5 |
| `hitz-whp/src/tests.rs` | Modify | 2, 6 |
| `hitz-daemon/src/lib.rs` | Modify | 3 |
| `hitz-daemon/src/vm_manager.rs` | **NEW** | 3 |
| `hitz-daemon/src/server.rs` | **NEW** | 4 |
| `hitz-daemon/src/router.rs` | **NEW** | 4 |
| `hitz-daemon/src/main.rs` | **NEW** | 4 |
| `hitz-daemon/Cargo.toml` | Modify | 3, 4 |
| `hitz-cli/src/pipe_client.rs` | **NEW** | 5 |
| `hitz-cli/Cargo.toml` | Modify | 5 |
| `Cargo.toml` (workspace) | Modify | 5 |
| `hitz-whp/src/tests.rs` | Modify | 6 |

## What We're NOT Building

- No TCP listener (Phase 7)
- No live serial streaming over pipe (Phase 7)
- No Windows service registration
- No multi-vCPU support
- No virtio-net
- No `hitz run` auto-starting an embedded daemon
