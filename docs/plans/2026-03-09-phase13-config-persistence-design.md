# Phase 13: Config Persistence — Design

## Goal

Survive daemon restarts: VM configurations persist to disk so `hitz vm list` shows previously-created VMs after a restart, and previously-running VMs are marked Stopped (not auto-restarted).

## Scope

**In:** Per-VM config + state files, write-through persistence, load-on-startup, `--state-dir` CLI flag.

**Out:** Auto-restart of running VMs on daemon restart, VM log persistence, metrics snapshot persistence.

## Storage Layout

```
%APPDATA%\hitz\vms\
  <vm-id>\
    config.json    ← full VmConfig (already serde-serializable)
    state.json     ← PersistedState { state, created_at, updated_at }
```

```rust
#[derive(Serialize, Deserialize)]
struct PersistedState {
    state:      VmState,  // Created | Stopped | Failed (never Running on disk at startup)
    created_at: u64,      // unix milliseconds
    updated_at: u64,
}
```

On daemon startup:
- Scan all subdirs of the state dir
- Load `config.json` + `state.json` per VM
- Any entry with `state = Running` is clamped to `Stopped`
- Missing `state.json` but valid `config.json` → treat as `Created`
- Missing/corrupt `config.json` → log warning, skip VM
- Non-VM subdirs → silently ignored

`delete_vm` removes the entire `<vm-id>/` directory.

## `StateStore` API

New file: `hitz-daemon/src/state_store.rs`

```rust
pub struct StateStore {
    base_dir: PathBuf,
}

impl StateStore {
    /// Creates `base_dir` if it does not exist.
    pub fn new(base_dir: PathBuf) -> Result<Self, DaemonError>;

    pub fn save_config(&self, id: &str, config: &VmConfig) -> Result<(), DaemonError>;
    pub fn save_state(&self, id: &str, state: VmState) -> Result<(), DaemonError>;
    pub fn delete(&self, id: &str) -> Result<(), DaemonError>;

    /// Returns (id, config, state) for all valid VM dirs. Corrupt entries are skipped.
    pub fn load_all(&self) -> Result<Vec<(String, VmConfig, VmState)>, DaemonError>;
}
```

## Write-Through Integration

`VmManager` gains `store: StateStore` and calls it at four mutation points:

| Operation | Store call |
|-----------|-----------|
| `create_vm` | `save_config` + `save_state(Created)` |
| `start_vm` (VM enters Running) | `save_state(Running)` |
| VM exits (Stopped / Failed) | `save_state(Stopped)` or `save_state(Failed)` |
| `delete_vm` | `store.delete` |

`VmManager::new` calls `store.load_all()` and populates the in-memory `HashMap`, clamping any `Running` entries to `Stopped`.

## CLI

`DaemonStartArgs` gains:

```rust
/// Directory for persisted VM state. Default: %APPDATA%\hitz\vms
#[arg(long)]
state_dir: Option<PathBuf>,
```

Default resolution in `run_daemon`:
```rust
let state_dir = args.state_dir.unwrap_or_else(|| {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hitz")
        .join("vms")
});
```

## Error Handling

| Situation | Behaviour |
|-----------|-----------|
| Corrupt `config.json` on load | Log warning, skip VM |
| Missing `state.json` | Treat as `Created` |
| Write failure | Propagate as `DaemonError::Io` |
| Non-VM subdir in state dir | Silently ignored |

## Files Changed

| File | Change |
|------|--------|
| `hitz-daemon/src/state_store.rs` | NEW: `StateStore` |
| `hitz-daemon/src/lib.rs` | `pub mod state_store` |
| `hitz-daemon/src/vm_manager.rs` | Add `StateStore` field, write-through calls, load on init |
| `hitz-cli/src/main.rs` | `--state-dir` flag, pass to `run_daemon` |
| `hitz-daemon/Cargo.toml` | Add `dirs = "5"` dep |

## Testing

**Unit (no WHP):**
- `StateStore` save+load roundtrip
- `load_all` with `Running` → `Stopped` clamp
- Corrupt `config.json` skipped, valid entries still loaded
- `delete` removes directory
- Missing `state.json` → `Created`

**Integration (VmManager, no WHP):**
- `create_vm` → reconstruct `VmManager` from same dir → VM appears in `list_vms`
- `start_vm` (fake hypervisor) → VM Running → reconstruct → state is `Stopped`
- `delete_vm` → reconstruct → VM absent
