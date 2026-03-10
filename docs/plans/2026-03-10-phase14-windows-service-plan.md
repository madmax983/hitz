# Phase 14 — Windows Service Registration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Register hitz daemon as a Windows Service via `hitz daemon install/remove`; same binary auto-detects SCM vs foreground.

**Architecture:** Add `windows-service = "0.6"` to `hitz-cli`. All SCM code lives in `hitz-cli/src/main.rs`. Refactor `run_daemon` to extract async body as `run_daemon_inner(args, shutdown: impl Future)` so both foreground (ctrl_c) and service (oneshot stop signal) share the same logic.

**Tech Stack:** `windows-service = "0.6"`, `tokio::sync::oneshot`, existing `hitz-daemon`, `clap`.

**Reference:** `docs/plans/2026-03-10-phase14-windows-service-design.md`

---

### Task 1: Add `windows-service` dep + `build_launch_args` helper + test

**Files:**
- Modify: `crates/hitz-cli/Cargo.toml`
- Modify: `crates/hitz-cli/src/main.rs`

**Step 1: Add dependency**

In `crates/hitz-cli/Cargo.toml`, add after `ctrlc`:
```toml
windows-service = "0.6"
```

**Step 2: Write the failing test**

Add at the bottom of the `#[cfg(test)]` block in `main.rs`:
```rust
#[test]
fn build_launch_args_round_trip() {
    // This test verifies that args baked into the service binary path
    // can be re-parsed back to identical DaemonStartArgs.
    let original = DaemonInstallArgs {
        pipe: r"\\.\pipe\hitz-test".to_string(),
        tcp_listen: None,
        verbose: false,
        otlp_endpoint: None,
        state_dir: Some(PathBuf::from(r"C:\hitz\vms")),
        auto_start: false,
        display_name: None,
        description: None,
    };
    let launch_args = build_launch_args(&original);
    // Re-parse as if SCM launched us with these args
    let mut argv = vec![std::ffi::OsString::from("hitz")];
    argv.extend(launch_args.into_iter().map(std::ffi::OsString::from));
    let cli = Cli::try_parse_from(argv).expect("re-parse failed");
    let Command::Daemon(DaemonCommand::Start(recovered)) = cli.command else {
        panic!("expected daemon start");
    };
    assert_eq!(recovered.pipe, original.pipe);
    assert_eq!(recovered.state_dir, original.state_dir);
    assert_eq!(recovered.verbose, original.verbose);
}
```

**Step 3: Run to verify it fails**
```bash
cd C:/Users/markm/hitz
cargo test -p hitz-cli build_launch_args_round_trip 2>&1 | tail -20
```
Expected: compile error — `DaemonInstallArgs`, `build_launch_args` don't exist yet.

**Step 4: Add `DaemonInstallArgs` struct and `DaemonCommand` variants**

In `main.rs`, replace the existing `DaemonCommand` enum:
```rust
/// Daemon management subcommands.
#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the daemon in the foreground.
    Start(DaemonStartArgs),
    /// Install hitz as a Windows service (requires admin).
    Install(Box<DaemonInstallArgs>),
    /// Remove the hitz Windows service (requires admin).
    Remove,
}
```

Add `DaemonInstallArgs` struct after `DaemonStartArgs`:
```rust
/// Arguments for `daemon install`.
#[derive(Parser)]
struct DaemonInstallArgs {
    /// Named pipe path baked into the service binary path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Optional TCP address to listen on (e.g. 127.0.0.1:8080).
    #[arg(long)]
    tcp_listen: Option<std::net::SocketAddr>,

    /// Verbose output.
    #[arg(short, long)]
    verbose: bool,

    /// OTLP gRPC collector endpoint.
    #[arg(long)]
    otlp_endpoint: Option<String>,

    /// Directory for persisted VM state.
    #[arg(long)]
    state_dir: Option<PathBuf>,

    /// Auto-start at boot (ServiceStartType::AutoStart). Default: manual start.
    #[arg(long)]
    auto_start: bool,

    /// Display name shown in Services MMC. Default: "Hitz MicroVM Daemon".
    #[arg(long)]
    display_name: Option<String>,

    /// Service description. Default: "Hyper-V microVM manager".
    #[arg(long)]
    description: Option<String>,
}
```

**Step 5: Add `build_launch_args` helper**

Add below the `DaemonInstallArgs` struct:
```rust
/// Build the `launch_arguments` vec that gets baked into the SCM registry entry.
///
/// The SCM will invoke: `hitz.exe daemon start <these args>`.
/// We include only non-default values to keep the binary path readable,
/// EXCEPT --pipe and --state-dir which are always explicit for clarity.
fn build_launch_args(args: &DaemonInstallArgs) -> Vec<String> {
    let mut v = vec!["daemon".to_string(), "start".to_string()];
    v.push("--pipe".to_string());
    v.push(args.pipe.clone());
    if let Some(addr) = args.tcp_listen {
        v.push("--tcp-listen".to_string());
        v.push(addr.to_string());
    }
    if args.verbose {
        v.push("--verbose".to_string());
    }
    if let Some(ref ep) = args.otlp_endpoint {
        v.push("--otlp-endpoint".to_string());
        v.push(ep.clone());
    }
    // Always include --state-dir explicitly so the service doesn't depend on APPDATA.
    let state_dir = args
        .state_dir
        .clone()
        .unwrap_or_else(|| resolve_state_dir(None));
    v.push("--state-dir".to_string());
    v.push(state_dir.to_string_lossy().into_owned());
    v
}
```

**Step 6: Run test to verify it passes**
```bash
cargo test -p hitz-cli build_launch_args_round_trip 2>&1 | tail -10
```
Expected: `test build_launch_args_round_trip ... ok`

**Step 7: Fix `Command::Daemon` match arm** in `main()` — add stubs for Install/Remove:
```rust
Command::Daemon(cmd) => match cmd {
    DaemonCommand::Start(ref args) => match run_daemon(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    },
    DaemonCommand::Install(ref args) => match install_service(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    },
    DaemonCommand::Remove => match remove_service() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    },
},
```

Add stub functions (will implement in Task 2):
```rust
fn install_service(_args: &DaemonInstallArgs) -> Result<()> {
    anyhow::bail!("not yet implemented")
}

fn remove_service() -> Result<()> {
    anyhow::bail!("not yet implemented")
}
```

**Step 8: Verify build**
```bash
cargo build -p hitz-cli 2>&1 | tail -20
```
Expected: compiles cleanly.

**Step 9: Commit**
```bash
cd C:/Users/markm/hitz
git add crates/hitz-cli/Cargo.toml crates/hitz-cli/src/main.rs Cargo.lock
git commit -m "feat(cli): DaemonInstallArgs, build_launch_args, Install/Remove stubs"
```

---

### Task 2: Implement `install_service`

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

**Context:** `windows-service` 0.6 API:
```rust
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceStatus, ServiceStatusHandle, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};
```

`ServiceManager::local_computer(None::<&str>, access)` opens the local SCM.
`manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)` registers the service.
`service.set_description(description)` sets the description string.

**Step 1: Add import block at top of main.rs**

After the existing `use` statements, add:
```rust
use std::ffi::OsString;
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};
```

**Step 2: Write integration test** (admin-gated, `#[ignore]`)

Add to the `#[cfg(test)]` block:
```rust
/// Requires admin and HITZ_TEST_SERVICE=1.
/// Run with: cargo test -p hitz-cli svc_install_and_remove -- --ignored
#[test]
#[ignore]
fn svc_install_and_remove() {
    if std::env::var("HITZ_TEST_SERVICE").is_err() {
        return;
    }
    // Clean up any leftover from a previous run
    let _ = remove_service();

    let args = DaemonInstallArgs {
        pipe: DEFAULT_PIPE.to_string(),
        tcp_listen: None,
        verbose: false,
        otlp_endpoint: None,
        state_dir: Some(PathBuf::from(r"C:\hitz\vms-test")),
        auto_start: false,
        display_name: Some("Hitz Test Service".to_string()),
        description: Some("Integration test".to_string()),
    };
    install_service(&args).expect("install failed");

    // Verify entry exists
    let mgr = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT,
    )
    .expect("open SCM");
    let svc = mgr.open_service("hitz", ServiceAccess::QUERY_STATUS);
    assert!(svc.is_ok(), "service not found after install");

    remove_service().expect("remove failed");

    // Verify entry gone
    let svc2 = mgr.open_service("hitz", ServiceAccess::QUERY_STATUS);
    assert!(svc2.is_err(), "service still exists after remove");
}

#[test]
#[ignore]
fn svc_install_idempotent_error() {
    if std::env::var("HITZ_TEST_SERVICE").is_err() {
        return;
    }
    let _ = remove_service();
    let args = DaemonInstallArgs {
        pipe: DEFAULT_PIPE.to_string(),
        tcp_listen: None,
        verbose: false,
        otlp_endpoint: None,
        state_dir: None,
        auto_start: false,
        display_name: None,
        description: None,
    };
    install_service(&args).expect("first install");
    let result = install_service(&args);
    assert!(result.is_err(), "second install should fail");
    assert!(
        result.unwrap_err().to_string().contains("already exists")
            || result.unwrap_err().to_string().contains("1073"),
        "expected service-exists error"
    );
    remove_service().expect("cleanup");
}

#[test]
#[ignore]
fn svc_remove_nonexistent() {
    if std::env::var("HITZ_TEST_SERVICE").is_err() {
        return;
    }
    let _ = remove_service(); // ensure not installed
    let result = remove_service();
    assert!(result.is_err(), "remove of non-existent should error");
}
```

**Step 3: Implement `install_service`**

Replace the stub:
```rust
fn install_service(args: &DaemonInstallArgs) -> Result<()> {
    use windows_service::{
        service::{
            ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
        },
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let exe = std::env::current_exe().context("failed to get current exe path")?;
    let launch_arguments: Vec<OsString> = build_launch_args(args)
        .into_iter()
        .map(OsString::from)
        .collect();

    let display_name = args
        .display_name
        .clone()
        .unwrap_or_else(|| "Hitz MicroVM Daemon".to_string());
    let description = args
        .description
        .clone()
        .unwrap_or_else(|| "Hyper-V microVM manager".to_string());

    let start_type = if args.auto_start {
        ServiceStartType::AutoStart
    } else {
        ServiceStartType::OnDemand
    };

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("failed to open SCM — run as Administrator")?;

    let service_info = ServiceInfo {
        name: OsString::from("hitz"),
        display_name: OsString::from(display_name),
        service_type: ServiceType::OWN_PROCESS,
        start_type,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments,
        dependencies: vec![],
        account_name: None,  // LocalSystem
        account_password: None,
    };

    let service = manager
        .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
        .context("failed to create service — already exists or access denied")?;

    service
        .set_description(description)
        .context("failed to set service description")?;

    println!("installed: hitz daemon as Windows service");
    if args.auto_start {
        println!("  start type: automatic (starts at boot)");
    } else {
        println!("  start type: manual — use `sc start hitz` to start");
    }
    Ok(())
}
```

**Step 4: Implement `remove_service`**

Replace the stub:
```rust
fn remove_service() -> Result<()> {
    use windows_service::{
        service::ServiceAccess,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .context("failed to open SCM — run as Administrator")?;

    let service = manager
        .open_service(
            "hitz",
            ServiceAccess::DELETE | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
        )
        .context("service 'hitz' not found — is it installed?")?;

    // Best-effort stop before delete (ignore errors — it may already be stopped).
    let _ = service.stop();

    service
        .delete()
        .context("failed to delete service")?;

    println!("removed: hitz Windows service");
    Ok(())
}
```

**Step 5: Build and verify**
```bash
cargo build -p hitz-cli 2>&1 | tail -20
cargo test -p hitz-cli build_launch_args_round_trip 2>&1 | tail -5
```
Expected: clean build, test passes.

**Step 6: Commit**
```bash
git add crates/hitz-cli/src/main.rs
git commit -m "feat(cli): implement install_service and remove_service"
```

---

### Task 3: Refactor `run_daemon` → `run_daemon_inner`

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

**Goal:** Extract the tokio async body into `run_daemon_inner(args, shutdown)` where `shutdown: impl Future<Output=()>`. Both foreground and service paths will call this.

**Step 1: Write the test first**

Add to `#[cfg(test)]` block:
```rust
#[tokio::test]
async fn run_daemon_inner_exits_on_shutdown() {
    // Verify that run_daemon_inner returns when the shutdown future resolves.
    // We don't start a real hypervisor — just verify the plumbing exits cleanly.
    // This is a structural smoke test; the WHP integration tests cover real boot.
    //
    // NOTE: This test cannot easily instantiate WhpHypervisor without real WHP.
    // Instead, verify the function signature compiles with a no-op future.
    // The real test is: `cargo build` succeeds and integration tests pass.
    let _ = async {
        // Compile-time check: the function exists and accepts the right types.
        let _: fn(
            &DaemonStartArgs,
            std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        ) -> _ = |_args, _shutdown| {
            run_daemon_inner(_args, _shutdown)
        };
    };
}
```

Actually this test is too hard to make useful without a real hypervisor. Skip it — the round-trip test and build are sufficient. Move to implementation.

**Step 1 (revised): Extract `run_daemon_inner`**

The current `run_daemon` function (lines ~430-507 in main.rs) has this structure:
```
run_daemon(args)
  setup telemetry
  setup tracing subscriber
  print startup messages
  rt.block_on(async {
    resolve state_dir
    create WhpHypervisor
    create VmManager
    create shutdown_tx/rx
    spawn server_handle
    ctrl_c().await   ← THIS IS THE SHUTDOWN TRIGGER
    send shutdown
    stop_all_and_wait
    await server_handle
  })
  rt.shutdown_timeout
  drop telemetry
```

Refactor to split into two functions:

```rust
/// Async body shared by foreground and service modes.
///
/// `shutdown` resolves when the caller wants the daemon to stop
/// (Ctrl+C in foreground, ServiceControl::Stop in service mode).
async fn run_daemon_inner(
    args: &DaemonStartArgs,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<()> {
    let state_dir = resolve_state_dir(args.state_dir.clone());
    eprintln!("hitz: state dir {}", state_dir.display());
    let hv = Arc::new(WhpHypervisor::new().context("WHP not available")?);
    let manager = hitz_daemon::VmManager::new(hv, state_dir)
        .context("failed to initialize VM state store")?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let server_mgr = manager.clone();
    let pipe = args.pipe.clone();
    let tcp = args.tcp_listen;
    let server_handle = tokio::spawn(async move {
        hitz_daemon::run_server(&pipe, tcp, server_mgr, shutdown_rx).await
    });

    // Wait for external shutdown signal (caller-provided future).
    shutdown.await;
    eprintln!("\nhitz: shutting down...");

    let _ = shutdown_tx.send(true);
    manager
        .stop_all_and_wait(std::time::Duration::from_secs(5))
        .await;
    let _ = server_handle.await;

    eprintln!("hitz: shutdown complete");
    Ok(())
}

/// Run the daemon in the foreground (existing entry point).
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    // ... telemetry setup unchanged ...

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    let result = rt.block_on(async {
        // Shutdown trigger: Ctrl+C
        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .context("failed to listen for Ctrl+C")
                .unwrap_or(());
        };
        run_daemon_inner(args, shutdown).await
    });

    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);
    result
}
```

**Step 2: Apply the refactor to `main.rs`**

Find the current `run_daemon` body (the `rt.block_on` async block) and extract it. The telemetry setup and tracing subscriber setup stay in `run_daemon`. Only the async inner body moves to `run_daemon_inner`.

Key: the `ctrl_c()` call becomes a passed-in future.

**Step 3: Verify build and tests**
```bash
cargo build -p hitz-cli 2>&1 | tail -20
cargo test -p hitz-cli 2>&1 | tail -20
```
Expected: clean build, all existing tests still pass.

**Step 4: Commit**
```bash
git add crates/hitz-cli/src/main.rs
git commit -m "refactor(cli): extract run_daemon_inner with generic shutdown future"
```

---

### Task 4: Add `define_windows_service!` macro and `service_main` callback

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

**Context:** The `define_windows_service!` macro must be called at module level (not inside a function). It generates a `ffi_service_main` function with the correct C ABI. The `service_main` callback runs inside the SCM dispatcher thread.

**Step 1: Add additional imports**

Add to the `use` block at the top of `main.rs`:
```rust
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
```

**Step 2: Place macro call**

After the `use` block, before `fn main()`, add at module level:
```rust
// Generate the FFI entry point for the Windows Service dispatcher.
// The SCM calls ffi_service_main; it delegates to service_main.
define_windows_service!(ffi_service_main, service_main);
```

**Step 3: Implement `service_main`**

Add after the macro call:
```rust
/// Entry point called by the SCM dispatcher.
///
/// Parses args (baked in at install time), registers the stop control handler,
/// reports service state, then runs the daemon body.
fn service_main(arguments: Vec<std::ffi::OsString>) {
    if let Err(e) = run_service(arguments) {
        eprintln!("hitz service error: {e:#}");
    }
}

fn run_service(arguments: Vec<std::ffi::OsString>) -> Result<()> {
    use std::time::Duration;

    // Channel: control handler (sync) → service body (async via block_on).
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let status_handle = service_control_handler::register(
        "hitz",
        move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        },
    )
    .context("failed to register service control handler")?;

    // Report: starting
    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })
        .context("set StartPending")?;

    // Re-parse the arguments baked in at install time.
    // arguments[0] is the exe path (or empty), rest are the CLI args.
    // We prepend "hitz" as argv[0] so clap sees a valid argv.
    let mut argv = vec![std::ffi::OsString::from("hitz")];
    // Skip first element (exe path provided by SCM) if it looks like a path.
    let rest = if arguments
        .first()
        .map(|a| a.to_string_lossy().contains('\\') || a.to_string_lossy().contains('/'))
        .unwrap_or(false)
    {
        &arguments[1..]
    } else {
        &arguments[..]
    };
    argv.extend_from_slice(rest);

    let cli = Cli::try_parse_from(argv).context("failed to parse service arguments")?;
    let Command::Daemon(DaemonCommand::Start(daemon_args)) = cli.command else {
        anyhow::bail!("service was not registered with 'daemon start' command");
    };

    // Report: running
    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .context("set Running")?;

    // Init telemetry (same as foreground path).
    let endpoint = resolve_otlp_endpoint(daemon_args.otlp_endpoint.clone());
    let telemetry = hitz_daemon::TelemetryGuard::init(endpoint);

    {
        use tracing_subscriber::Layer as _;
        let mut layers: Vec<
            Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
        > = Vec::new();
        if daemon_args.verbose {
            let fmt = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::filter::EnvFilter::new("hitz=debug"))
                .boxed();
            layers.push(fmt);
        }
        if let Some(otel) = telemetry.tracing_layer() {
            layers.push(otel.boxed());
        }
        // Best-effort: if subscriber already set (e.g. tests), ignore error.
        let _ = tracing_subscriber::registry().with(layers).try_init();
    }

    let rt =
        tokio::runtime::Runtime::new().context("failed to create tokio runtime in service")?;

    let result = rt.block_on(async {
        // Shutdown trigger: SCM Stop signal via mpsc channel.
        let shutdown = async move {
            // Bridge sync mpsc to async: poll in a spawn_blocking.
            tokio::task::spawn_blocking(move || {
                let _ = stop_rx.recv();
            })
            .await
            .unwrap_or(());
        };
        run_daemon_inner(&daemon_args, shutdown).await
    });

    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);

    let exit_code = if result.is_ok() { 0 } else { 1 };

    // Report: stopped
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(exit_code),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    result
}
```

**Step 4: Build and verify**
```bash
cargo build -p hitz-cli 2>&1 | tail -30
```
Expected: clean build. Fix any import or type errors.

**Step 5: Commit**
```bash
git add crates/hitz-cli/src/main.rs
git commit -m "feat(cli): add define_windows_service macro and service_main callback"
```

---

### Task 5: Wire auto-detection in `run_daemon`

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

**Goal:** `hitz daemon start` tries SCM dispatcher first. Falls back to foreground if not running under SCM.

`ERROR_FAILED_SERVICE_CONTROLLER_CONNECT` = Windows error code 1063 (0x427).

**Step 1: Modify `run_daemon` to try service dispatch first**

At the very top of `run_daemon`, before telemetry setup, add:
```rust
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    // Auto-detect: if launched by the Service Control Manager, enter service mode.
    // service_dispatcher::start() returns ERROR_FAILED_SERVICE_CONTROLLER_CONNECT (1063)
    // when NOT running under SCM — that's our signal to fall through to foreground mode.
    match service_dispatcher::start("hitz", ffi_service_main) {
        Ok(()) => {
            // Service ran and exited cleanly.
            return Ok(());
        }
        Err(windows_service::Error::Winapi(ref e))
            if e.raw_os_error() == Some(1063) =>
        {
            // Not running as a service — continue to foreground mode below.
        }
        Err(e) => {
            return Err(anyhow::Error::new(e).context("service dispatcher error"));
        }
    }

    // ── Foreground mode ──────────────────────────────────────────────────────
    use tracing_subscriber::prelude::*;
    // ... rest of existing run_daemon body unchanged ...
```

**Step 2: Build and verify**
```bash
cargo build -p hitz-cli 2>&1 | tail -20
cargo test -p hitz-cli 2>&1 | tail -20
```
Expected: clean build, all unit tests pass.

**Step 3: Manual smoke test** (foreground still works)
```bash
cargo run -p hitz-cli -- daemon start --verbose &
# Should print: "hitz: daemon listening on \\.\pipe\hitz"
# (Not enter service mode since we're not under SCM)
# Ctrl+C to stop
```

**Step 4: Commit**
```bash
git add crates/hitz-cli/src/main.rs
git commit -m "feat(cli): wire service_dispatcher auto-detection in run_daemon"
```

---

### Task 6: Final verification and cleanup

**Step 1: Run full test suite**
```bash
cd C:/Users/markm/hitz
cargo test --workspace 2>&1 | tail -30
```
Expected: all non-ignored tests pass.

**Step 2: Clippy**
```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -30
```
Fix any warnings. Common issues:
- Unused imports in service path
- `allow(clippy::expect_used)` needed in service_main (it runs outside normal error context)

**Step 3: Verify help text**
```bash
cargo run -p hitz-cli -- daemon --help
```
Expected output includes `install` and `remove` subcommands.

```bash
cargo run -p hitz-cli -- daemon install --help
```
Expected: shows all flags including `--auto`, `--display-name`, `--description`.

**Step 4: Update MEMORY.md**

Add Phase 14 entry to `C:\Users\markm\.claude\projects\C--Users-markm-hitz\memory\MEMORY.md`:
```
## Phase 14 Key Files
- `hitz-cli/Cargo.toml` — `windows-service = "0.6"` dep
- `hitz-cli/src/main.rs` — `DaemonInstallArgs`, `build_launch_args`, `install_service`, `remove_service`, `define_windows_service!(ffi_service_main, service_main)`, `run_service`, `run_daemon_inner` (shared async body)
- `run_daemon` auto-detects SCM via `service_dispatcher::start` → `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT` (1063) means foreground
- Service stop: sync `mpsc::channel` bridges SCM control handler to tokio via `spawn_blocking`
- `build_launch_args` always includes `--pipe` and `--state-dir` explicitly (resolved at install time)
- Integration tests: `HITZ_TEST_SERVICE=1` env var gate, `#[ignore]`
```

**Step 5: Final commit**
```bash
git add C:/Users/markm/.claude/projects/C--Users-markm-hitz/memory/MEMORY.md
git commit -m "docs: update MEMORY.md for Phase 14 Windows service"
```

---

## Quick Reference: Key Types

```rust
// Crate: windows-service = "0.6"
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl,
        ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

// ERROR_FAILED_SERVICE_CONTROLLER_CONNECT = 1063
// ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE  (for install)
// ServiceAccess::DELETE | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS  (for remove)
// ServiceAccess::CHANGE_CONFIG  (after create, to set description)
```
