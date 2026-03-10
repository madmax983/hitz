# Phase 14 — Windows Service Registration

**Date:** 2026-03-10
**Status:** Approved

## Goal

Allow the hitz daemon to run as a Windows Service — auto-starts at boot, survives user logoff, managed via SCM. No new binary; same `hitz.exe` serves both foreground and service modes.

## New CLI Commands

Under `hitz daemon`:

```
hitz daemon install   [options]   # register with SCM (requires admin)
hitz daemon remove                # unregister (requires admin)
hitz daemon start     [options]   # existing: auto-detects service context, falls back to foreground
```

### `hitz daemon install` flags

| Flag | Default | Notes |
|------|---------|-------|
| `--pipe` | `\\.\pipe\hitz` | Baked into service binary path |
| `--tcp-listen` | none | Optional TCP listener |
| `--state-dir` | `%APPDATA%\hitz\vms` | VM state directory |
| `--otlp-endpoint` | none | OTel collector |
| `--verbose` | false | Enable debug logging |
| `--auto` | false | `AutoStart` vs `OnDemand` |
| `--display-name` | `"Hitz MicroVM Daemon"` | SCM display name |
| `--description` | `"Hyper-V microVM manager"` | SCM description string |

The install command writes the service binary path as:
```
C:\...\hitz.exe daemon start --pipe \\.\pipe\hitz --state-dir C:\...\hitz\vms [...]
```

### `hitz daemon remove`

No flags. Opens SCM, stops the service if running, deletes the entry.

Both commands require administrator privileges. On `AccessDenied`, surface the OS error with a hint: `"run as Administrator"`.

---

## Architecture

### Crate changes

- **`hitz-cli`**: all SCM interaction lives here. Adds `windows-service = "0.6"` dep.
- **`hitz-daemon`**: no new deps, no structural changes.

### Auto-detection flow

`hitz daemon start` tries the SCM dispatcher unconditionally:

```
run_daemon(args)
  │
  ├─ service_dispatcher::start("hitz", ffi_service_main)
  │    ├─ Ok(())  ──────────────────── service ran and exited → return Ok
  │    ├─ Err(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT)
  │    │         ────────────────────  not under SCM → fall through
  │    └─ Err(other) ──────────────── fatal, bubble up
  │
  └─ run_daemon_foreground(args)    ← existing logic
```

`define_windows_service!(ffi_service_main, service_main)` macro is called once at the top of `hitz-cli/src/main.rs`.

### Service entry point (`service_main`)

```
service_main(arguments: Vec<OsString>)
  1. Register control handler
       ServiceControl::Stop → send on tokio oneshot channel
       ServiceControl::Interrogate → NoError
  2. Report ServiceState::StartPending
  3. Re-parse arguments as CLI args via Cli::try_parse_from
       → recover DaemonStartArgs baked in at install time
  4. Report ServiceState::Running
  5. Run run_daemon_inner(args, shutdown_rx)
  6. Report ServiceState::Stopped
```

### Shared async body refactor

Extract the tokio async body from `run_daemon` into:

```rust
async fn run_daemon_inner(
    args: &DaemonStartArgs,
    shutdown: impl Future<Output = ()>,
) -> Result<()>
```

| Caller | `shutdown` future |
|--------|-------------------|
| Foreground | `tokio::signal::ctrl_c()` |
| Service | `oneshot::Receiver` resolved on `ServiceControl::Stop` |

The rest of the body (telemetry init, `VmManager` setup, server spawn, `stop_all_and_wait`) is identical for both paths.

---

## File Changes

| File | Change |
|------|--------|
| `crates/hitz-cli/Cargo.toml` | add `windows-service = "0.6"` |
| `crates/hitz-cli/src/main.rs` | `DaemonCommand::{Install,Remove}` variants; `define_windows_service!` macro; `service_main` callback; `install_service` / `remove_service` fns; refactor `run_daemon` → `run_daemon_inner` |

---

## Testing

### Unit tests (no admin, no SCM)

- **Arg round-trip**: construct `DaemonInstallArgs` → build `launch_arguments` vec → `Cli::try_parse_from` → assert all fields survive the round-trip
- **`--auto` flag**: verify `ServiceStartType::AutoStart` vs `OnDemand` mapping
- **Missing `--pipe`**: defaults applied correctly in launch args

### Integration tests (`#[ignore]`, gated on `HITZ_TEST_SERVICE=1`, require admin)

- `svc_install_and_remove` — install → query SCM entry exists → remove → query gone
- `svc_install_idempotent_error` — install twice → `ERROR_SERVICE_EXISTS`, not panic
- `svc_remove_nonexistent` — remove when not installed → clean error

### Manual verification

- Start installed service via `sc start hitz`, verify named pipe responds to `hitz vm list`
- Stop via `sc stop hitz`, verify graceful shutdown (VMs stopped, logs flushed)
- Reboot with `--auto`, verify daemon started before login

---

## Non-Goals

- Auto-elevation (UAC prompt) — user must run as admin manually
- Service recovery actions (auto-restart on crash) — deferred; use SCM UI or `sc failure` post-install
- Windows Event Log integration — deferred; OTLP covers observability for now
