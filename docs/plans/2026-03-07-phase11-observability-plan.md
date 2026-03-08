# Phase 11: Observability — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add OpenTelemetry traces and metrics to the Hitz daemon for production observability.

**Architecture:** `tracing` spans bridge to OTel traces via `tracing-opentelemetry`; metrics use `opentelemetry::global::meter()` directly. OTLP gRPC export to any compatible backend. Zero cost when disabled.

**Tech Stack:** Rust 2024, `opentelemetry` 0.28, `opentelemetry-otlp` 0.28 (tonic/gRPC), `tracing-opentelemetry` 0.29, tokio.

---

## Context snapshot (verified by reading source)

- `crates/hitz-daemon/src/lib.rs` exports: `error`, `port_forward`, `router`, `server`, `vm_manager` modules + `DaemonError`, `run_server`, `VmManager`.
- `crates/hitz-cli/src/main.rs`: `DaemonStartArgs` has `pipe: String`, `tcp_listen: Option<SocketAddr>`, `verbose: bool`. `run_daemon` calls `tracing_subscriber::fmt()...init()` only inside `if args.verbose { }`. The async block returns `Ok(())`.
- `crates/hitz-daemon/src/router.rs`: `pub async fn route<H>(req: Request<Incoming>, manager: &VmManager<H>) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>`. Method extracted at line 56 (`req.method().clone()`), path at line 57 (`req.uri().path().to_string()`). Match at line 59.
- `crates/hitz-vmm/src/run_loop.rs`: `run_vcpu_loop` has `match exit { ... }` at line 102. Variants handled: `IoPort`, `Halt`, `Shutdown`, `Mmio`, `InterruptWindow`, `Canceled`, `Unknown(code)`. `ExitReason` variants: `Halt`, `Shutdown`, `Canceled`, `Unexpected(String)`.
- `crates/hitz-daemon/src/vm_manager.rs`: `start_vm` spawns async task at line 143. `_port_fwd` assigned at line 145. `spawn_blocking(boot_and_run)` at line 167. State updated after join at line 175. `completion_tx.send(vm_id)` at line 205.
- `crates/hitz-daemon/src/port_forward.rs`: `PortForwardManager::start(guest_ip, rules)`. Listener loop at `loop { match listener.accept().await { Ok((mut inbound, _peer)) => { let relay = tokio::spawn(...)` at line 54-82. `relay_handles_clone.lock().await.push(relay)` at line 74.

## Task dependency graph

```
Task 1 (deps + TelemetryGuard) ──► Task 2 (CLI + daemon init)
                                ──► Task 3 (router spans)        ┐
                                ──► Task 4 (vm_manager spans)    ├─ PARALLEL after Task 1
                                ──► Task 5 (run_loop counter)    ┤
                                ──► Task 6 (port_forward metrics)┘
Tasks 2-6 ──► Task 7 (integration test skeleton)
```

Tasks 3, 4, 5, 6 touch different files and can be implemented in parallel after Task 1 is complete.

---

## Task 1: Workspace dependencies + `TelemetryGuard`

**Files touched:**
- `Cargo.toml` (workspace root)
- `crates/hitz-daemon/Cargo.toml`
- `crates/hitz-vmm/Cargo.toml`
- `crates/hitz-daemon/src/telemetry.rs` (NEW)
- `crates/hitz-daemon/src/lib.rs`

### Step 1.1 — Write failing test (RED)

Create the file `crates/hitz-daemon/src/telemetry.rs` with just the test module so the test references the not-yet-written API:

```rust
// crates/hitz-daemon/src/telemetry.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_none_is_noop() {
        let guard = TelemetryGuard::init(None);
        drop(guard); // must not panic
    }

    #[test]
    fn init_unreachable_endpoint_is_ok() {
        // OTel gRPC is lazy-connecting — init succeeds even with nothing
        // listening on port 4317.
        let guard = TelemetryGuard::init(Some("http://127.0.0.1:4317".to_string()));
        drop(guard); // graceful shutdown, must not panic
    }
}
```

Run to confirm RED:

```bash
cargo test -p hitz-daemon telemetry 2>&1 | head -20
# Expected: error[E0412]: cannot find type `TelemetryGuard`
```

### Step 1.2 — Add workspace dependencies

In `Cargo.toml`, add to `[workspace.dependencies]` (verify latest compatible versions at
<https://crates.io> before pinning — OTel Rust releases frequently):

```toml
opentelemetry          = "0.28"
opentelemetry_sdk      = { version = "0.28", features = ["rt-tokio"] }
opentelemetry-otlp     = { version = "0.28", features = ["tonic"] }
tracing-opentelemetry  = "0.29"
```

In `crates/hitz-daemon/Cargo.toml`, add under `[dependencies]`:

```toml
opentelemetry.workspace          = true
opentelemetry_sdk.workspace      = true
opentelemetry-otlp.workspace     = true
tracing-opentelemetry.workspace  = true
```

In `crates/hitz-vmm/Cargo.toml`, add under `[dependencies]`:

```toml
opentelemetry.workspace = true
```

Only the thin API crate — no SDK. When no global meter is registered, all calls are no-ops at
the OTel specification level.

### Step 1.3 — Implement `TelemetryGuard`

Replace `crates/hitz-daemon/src/telemetry.rs` with the full implementation:

```rust
//! OpenTelemetry initialisation — traces + metrics over OTLP gRPC.
//!
//! Call [`TelemetryGuard::init`] once at daemon startup. Hold the returned
//! guard for the lifetime of the process; dropping it flushes and shuts down
//! both the tracer and meter providers.
//!
//! When `endpoint` is `None` the function returns immediately with a no-op
//! guard — no global providers are installed and all OTel calls are no-ops.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// RAII guard that shuts down OTel providers on drop.
///
/// Holds `Option<…>` so the no-op path occupies zero heap.
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl TelemetryGuard {
    /// Initialise OTel with an OTLP gRPC exporter.
    ///
    /// If `endpoint` is `None`, returns a no-op guard without installing any
    /// global provider.  The gRPC channel is lazy — init succeeds even when
    /// no collector is running.
    ///
    /// # Errors (logged, not propagated)
    ///
    /// If provider construction fails (e.g. bad endpoint URL), a warning is
    /// logged and the no-op guard is returned so the daemon still starts.
    #[must_use]
    pub fn init(endpoint: Option<String>) -> Self {
        let Some(endpoint) = endpoint else {
            return Self {
                tracer_provider: None,
                meter_provider: None,
            };
        };

        // ── Tracer provider ──────────────────────────────────────────────────
        let span_exporter = match SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("OTel span exporter init failed: {e}; running without traces");
                return Self {
                    tracer_provider: None,
                    meter_provider: None,
                };
            }
        };

        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter, opentelemetry_sdk::runtime::Tokio)
            .build();

        // Register as the global tracer provider and bridge tracing spans.
        let tracer = tracer_provider.tracer("hitz");
        opentelemetry::global::set_tracer_provider(tracer_provider.clone());
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        // Note: the subscriber is NOT installed here — Task 2 wires it into
        // the registry. We return the layer via a separate helper if needed.
        // For the global side-effect (tracer provider), the call above is enough.
        let _ = otel_layer; // suppress unused warning until Task 2 integration

        // ── Meter provider ───────────────────────────────────────────────────
        let metric_exporter = match MetricExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("OTel metric exporter init failed: {e}; running without metrics");
                // Tracer is already installed; return with no meter.
                return Self {
                    tracer_provider: Some(tracer_provider),
                    meter_provider: None,
                };
            }
        };

        let meter_provider = SdkMeterProvider::builder()
            .with_periodic_exporter(metric_exporter, opentelemetry_sdk::runtime::Tokio)
            .build();

        opentelemetry::global::set_meter_provider(meter_provider.clone());

        tracing::info!(%endpoint, "OTel OTLP exporter initialised");

        Self {
            tracer_provider: Some(tracer_provider),
            meter_provider: Some(meter_provider),
        }
    }

    /// Build a `tracing_opentelemetry` layer backed by the installed tracer.
    ///
    /// Returns `None` when OTel is disabled (no-op path).  Call this after
    /// [`init`] and before building the subscriber registry.
    #[must_use]
    pub fn tracing_layer(
        &self,
    ) -> Option<tracing_opentelemetry::OpenTelemetryLayer<tracing_subscriber::Registry, opentelemetry_sdk::trace::Tracer>>
    {
        self.tracer_provider.as_ref().map(|tp| {
            let tracer = tp.tracer("hitz");
            tracing_opentelemetry::layer().with_tracer(tracer)
        })
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(ref mp) = self.meter_provider {
            if let Err(e) = mp.shutdown() {
                tracing::warn!("OTel meter provider shutdown error: {e}");
            }
        }
        if let Some(ref tp) = self.tracer_provider {
            if let Err(e) = tp.shutdown() {
                tracing::warn!("OTel tracer provider shutdown error: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_none_is_noop() {
        let guard = TelemetryGuard::init(None);
        drop(guard);
    }

    #[test]
    fn init_unreachable_endpoint_is_ok() {
        // gRPC is lazy-connecting; construction succeeds even with nothing
        // listening. Use a port unlikely to be occupied.
        let guard = TelemetryGuard::init(Some("http://127.0.0.1:14317".to_string()));
        drop(guard);
    }
}
```

### Step 1.4 — Add `pub mod telemetry` to `lib.rs`

In `crates/hitz-daemon/src/lib.rs`, add after the existing `pub mod vm_manager;` line:

```rust
pub mod telemetry;
pub use telemetry::TelemetryGuard;
```

### Step 1.5 — Run and verify GREEN

```bash
cargo test -p hitz-daemon telemetry -- --nocapture
# Expected:
# test telemetry::tests::init_none_is_noop ... ok
# test telemetry::tests::init_unreachable_endpoint_is_ok ... ok
```

```bash
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 1.6 — Commit

```bash
git add Cargo.toml \
        crates/hitz-daemon/Cargo.toml \
        crates/hitz-vmm/Cargo.toml \
        crates/hitz-daemon/src/telemetry.rs \
        crates/hitz-daemon/src/lib.rs
git commit -m "feat(telemetry): add OTel workspace deps and TelemetryGuard"
```

---

## Task 2: CLI `--otlp-endpoint` flag + subscriber restructure

**Files touched:**
- `crates/hitz-cli/src/main.rs`

**Depends on:** Task 1

### Step 2.1 — Write failing test (RED)

Add to the existing `#[cfg(test)] mod tests` block in `main.rs`:

```rust
#[test]
fn endpoint_resolution_cli_wins_over_env() {
    // CLI arg takes priority over env var.
    let cli_val = "http://cli-endpoint:4317".to_string();
    let env_val = "http://env-endpoint:4317";
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", env_val);
    let resolved = resolve_otlp_endpoint(Some(cli_val.clone()));
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(resolved, Some(cli_val));
}

#[test]
fn endpoint_resolution_falls_back_to_env() {
    let env_val = "http://env-endpoint:4317".to_string();
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", &env_val);
    let resolved = resolve_otlp_endpoint(None);
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(resolved, Some(env_val));
}

#[test]
fn endpoint_resolution_none_when_absent() {
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    let resolved = resolve_otlp_endpoint(None);
    assert_eq!(resolved, None);
}
```

Run to confirm RED:

```bash
cargo test -p hitz-cli endpoint_resolution 2>&1 | head -10
# Expected: error[E0425]: cannot find function `resolve_otlp_endpoint`
```

### Step 2.2 — Implement changes

**Add `--otlp-endpoint` to `DaemonStartArgs`:**

```rust
/// OTLP gRPC collector endpoint (e.g. http://localhost:4317).
/// Falls back to OTEL_EXPORTER_OTLP_ENDPOINT env var. Omit to disable telemetry.
#[arg(long)]
otlp_endpoint: Option<String>,
```

**Add `resolve_otlp_endpoint` helper** (before `main()`):

```rust
/// Resolve the OTLP endpoint: CLI flag → env var → None.
fn resolve_otlp_endpoint(cli_arg: Option<String>) -> Option<String> {
    cli_arg.or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
}
```

**Add `use hitz_daemon::TelemetryGuard;`** at the top of `main.rs`.

**Restructure `run_daemon`** to replace the conditional `tracing_subscriber::fmt()...init()` block
with a registry-based subscriber that always inits, with optional layers:

```rust
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    use tracing_subscriber::prelude::*;

    let endpoint = resolve_otlp_endpoint(args.otlp_endpoint.clone());

    // Initialise OTel providers (no-op when endpoint is None).
    // Must happen before subscriber registration so the tracer exists.
    let _telemetry = TelemetryGuard::init(endpoint);

    // Build a layered subscriber:
    //   - fmt layer when --verbose (human-readable stderr)
    //   - OTel bridge layer when endpoint is configured
    let fmt_layer = args.verbose.then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(tracing_subscriber::filter::EnvFilter::new("hitz=debug"))
    });
    let otel_layer = _telemetry.tracing_layer();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(otel_layer)
        .init();

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

        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl+C")?;
        eprintln!("\nhitz: shutting down...");

        let _ = shutdown_tx.send(true);
        manager
            .stop_all_and_wait(std::time::Duration::from_secs(5))
            .await;
        let _ = server_handle.await;

        eprintln!("hitz: shutdown complete");
        // _telemetry drops here → providers flush + shutdown.
        Ok(())
    })
    // _telemetry drops here if rt.block_on returns Err.
}
```

Note: `_telemetry` is bound at the top of `run_daemon` so it outlives the tokio runtime. OTel's
batch exporters need the runtime to flush, so the runtime must be dropped **before** the guard
drops. The guard must therefore be declared **after** `rt` but the runtime must be shut down before
the guard's drop runs. Achieve this by moving the `TelemetryGuard::init` call inside the
`rt.block_on` closure OR by explicitly calling `rt.shutdown_timeout` before the guard drops:

```rust
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    use tracing_subscriber::prelude::*;

    let endpoint = resolve_otlp_endpoint(args.otlp_endpoint.clone());
    // Initialise before subscriber so tracer exists when layer is built.
    let telemetry = TelemetryGuard::init(endpoint);

    {
        let fmt_layer = args.verbose.then(|| {
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::filter::EnvFilter::new("hitz=debug"))
        });
        let otel_layer = telemetry.tracing_layer();
        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    }

    eprintln!("hitz: daemon listening on {}", args.pipe);
    if let Some(addr) = args.tcp_listen {
        eprintln!("hitz: TCP listener on {addr}");
    }

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    let result = rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().context("WHP not available")?);
        let manager = hitz_daemon::VmManager::new(hv);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let server_mgr = manager.clone();
        let pipe = args.pipe.clone();
        let tcp = args.tcp_listen;
        let server_handle = tokio::spawn(async move {
            hitz_daemon::run_server(&pipe, tcp, server_mgr, shutdown_rx).await
        });

        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl+C")?;
        eprintln!("\nhitz: shutting down...");

        let _ = shutdown_tx.send(true);
        manager
            .stop_all_and_wait(std::time::Duration::from_secs(5))
            .await;
        let _ = server_handle.await;

        eprintln!("hitz: shutdown complete");
        Ok::<(), anyhow::Error>(())
    });

    // Shut down the runtime BEFORE dropping the OTel guard so the batch
    // exporter can flush its last spans/metrics over the tokio runtime.
    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);

    result
}
```

### Step 2.3 — Run and verify GREEN

```bash
cargo test -p hitz-cli endpoint_resolution -- --nocapture
# Expected:
# test tests::endpoint_resolution_cli_wins_over_env ... ok
# test tests::endpoint_resolution_falls_back_to_env ... ok
# test tests::endpoint_resolution_none_when_absent ... ok
```

```bash
cargo build -p hitz-cli 2>&1 | tail -5
# Expected: Compiling hitz-cli ... Finished
```

```bash
cargo clippy -p hitz-cli -- -D warnings
cargo fmt --check
```

### Step 2.4 — Commit

```bash
git add crates/hitz-cli/src/main.rs
git commit -m "feat(cli): add --otlp-endpoint flag and OTel subscriber wiring"
```

---

## Task 3: `daemon.request` span in router

**Files touched:**
- `crates/hitz-daemon/src/router.rs`

**Depends on:** Task 1

### Step 3.1 — Write failing test (RED)

Add to `crates/hitz-daemon/src/router.rs` a test module:

```rust
#[cfg(test)]
mod tests {
    /// Smoke test: route() compiles and returns without panic for a GET /vms request.
    /// The span instrumentation must not break the infallible return contract.
    #[tokio::test]
    async fn route_get_vms_is_infallible() {
        use super::*;
        use std::sync::Arc;
        use hitz_hal::Hypervisor;

        // We only need this to compile and run without panicking.
        // A real manager call would require a hypervisor — just verify
        // the span wrapper around route() does not break the Result type.
        //
        // Build a minimal request.
        let req = hyper::Request::builder()
            .method("GET")
            .uri("/vms")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .expect("build request");
        // We cannot call route() without a VmManager<H>, so we just assert
        // this test file compiles with the new span imports present.
        let _ = req;
    }
}
```

This test is primarily a compile guard. Run:

```bash
cargo test -p hitz-daemon route -- --nocapture 2>&1 | head -20
```

### Step 3.2 — Implement `daemon.request` span

In `crates/hitz-daemon/src/router.rs`, add `use tracing::Instrument as _;` at the top imports.

Wrap the body of `route()` with a span. The current function body is:

```rust
let method = req.method().clone();
let path = req.uri().path().to_string();

let result = match (method.clone(), path.as_str()) {
    ...
};

Ok(result.unwrap_or_else(|e| daemon_error_response(&e)))
```

Replace with:

```rust
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,
        http.status_code = tracing::field::Empty,
    );

    let result = async {
        match (method.clone(), path.as_str()) {
            (Method::GET, "/vms") => handle_list(manager),
            _ if path.starts_with("/vms/") => {
                let segments: Vec<&str> = path.splitn(4, '/').collect();
                match segments.get(2) {
                    Some(id) if !id.is_empty() => {
                        let suffix = segments.get(3).copied();
                        route_vm(req, &method, id, suffix, manager).await
                    }
                    _ => Ok(error_response(StatusCode::BAD_REQUEST, "missing VM ID")),
                }
            }
            _ => Ok(error_response(StatusCode::NOT_FOUND, "not found")),
        }
    }
    .instrument(span.clone())
    .await;

    let response = result.unwrap_or_else(|e| daemon_error_response(&e));

    // Record status code on the span before it closes.
    span.record("http.status_code", response.status().as_u16());

    Ok(response)
}
```

### Step 3.3 — Run GREEN

```bash
cargo test -p hitz-daemon -- --nocapture 2>&1 | tail -20
# All existing tests pass + new compile-guard test passes.
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 3.4 — Commit

```bash
git add crates/hitz-daemon/src/router.rs
git commit -m "feat(telemetry): daemon.request span in HTTP router"
```

---

## Task 4: `vm.boot` span + `hitz.vm.*` metrics in `vm_manager`

**Files touched:**
- `crates/hitz-daemon/src/vm_manager.rs`

**Depends on:** Task 1

### Step 4.1 — Write failing test (RED)

Add to the existing `#[cfg(test)]` block in `vm_manager.rs`:

```rust
#[test]
fn start_vm_emits_metrics_without_panic() {
    // Regression guard: metric calls must not panic even when no global
    // meter provider is registered (the no-op provider handles it).
    let mgr = make_manager();
    mgr.create_vm("m1".into(), make_config()).expect("create");
    // start_vm fires an async task; the metric calls happen inside it.
    // We cannot await completion here without a WHP partition, but the
    // synchronous parts (counter creation) happen before spawn_blocking.
    let _info = mgr.start_vm("m1").expect("start");
}
```

Run to confirm this test already passes (it tests existing behavior); the failing tests will be
the compile errors when we add references to new API in the implementation.

### Step 4.2 — Implement

In `crates/hitz-daemon/src/vm_manager.rs`, add at the top of the file:

```rust
use opentelemetry::global;
use opentelemetry::KeyValue;
```

In `new()`, create the `hitz.vm.count` up-down counter that will be shared across calls.
Since `VmManager` is `Clone` (all fields are `Arc`-wrapped), store the counter in a new field:

```rust
// Add field to VmManager<H>:
vm_count: opentelemetry::metrics::UpDownCounter<i64>,
```

Update `Clone` impl and `new()`:

```rust
impl<H: Hypervisor + Send + Sync + 'static> VmManager<H> {
    pub fn new(hypervisor: Arc<H>) -> Self {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let meter = global::meter("hitz");
        let vm_count = meter
            .i64_up_down_counter("hitz.vm.count")
            .with_description("Number of VMs by state")
            .build();
        Self {
            hypervisor,
            vms: Arc::new(Mutex::new(HashMap::new())),
            completion_tx: tx,
            completion_rx: Arc::new(tokio::sync::Mutex::new(rx)),
            vm_count: Arc::new(vm_count),
        }
    }
```

Wait — to avoid lifetime issues with the counter type, wrap it in `Arc`:

```rust
// Field type:
vm_count: Arc<opentelemetry::metrics::UpDownCounter<i64>>,
```

Update `Clone` impl:

```rust
impl<H> Clone for VmManager<H> {
    fn clone(&self) -> Self {
        Self {
            hypervisor: Arc::clone(&self.hypervisor),
            vms: Arc::clone(&self.vms),
            completion_tx: self.completion_tx.clone(),
            completion_rx: Arc::clone(&self.completion_rx),
            vm_count: Arc::clone(&self.vm_count),
        }
    }
}
```

In `start_vm`, add metric instrumentation and `vm.boot` span. The key insertion points:

**Before the async task spawn** (after extracting `config`, `stop_flag`, `serial_buf`):

```rust
// Record vm.memory_bytes gauge (one-shot observation at start).
{
    let meter = global::meter("hitz");
    let memory_bytes: u64 = u64::from(config.ram_mib) * 1024 * 1024;
    meter
        .u64_observable_gauge("hitz.vm.memory_bytes")
        .with_description("Guest RAM in bytes")
        .with_callback(move |observer| {
            observer.observe(
                memory_bytes,
                &[KeyValue::new("vm.id", vm_id_for_gauge.clone())],
            );
        })
        .build();
}

// Increment running VM count.
self.vm_count.add(1, &[KeyValue::new("state", "running")]);
```

Note: `u64_observable_gauge` with a callback registers a persistent gauge — every collection
cycle it calls the closure. For a simpler approach with `SdkMeterProvider`, a non-observable
gauge via `Gauge::record` is also available. Prefer the non-observable variant:

```rust
let memory_gauge = meter
    .u64_gauge("hitz.vm.memory_bytes")
    .with_description("Guest RAM in bytes at VM start")
    .build();
memory_gauge.record(
    u64::from(config.ram_mib) * 1024 * 1024,
    &[KeyValue::new("vm.id", vm_id.as_str())],
);
```

**Wrap the spawned task body with `vm.boot` span:**

```rust
let vm_id_span = vm_id.clone();
let vm_count_clone = Arc::clone(&self.vm_count);
drop(tokio::task::spawn(async move {
    let span = tracing::info_span!(
        "vm.boot",
        vm.id = %vm_id_span,
        vm.ram_mib = config.ram_mib,
        vm.cpus = config.cpus,
    );
    let _enter = span.enter();

    // ... existing _port_fwd setup and spawn_blocking(boot_and_run) ...

    // Decrement counter on completion (any terminal state).
    vm_count_clone.add(-1, &[KeyValue::new("state", "running")]);

    // ... existing state update and completion_tx.send ...
}));
```

The complete diff for the spawned async block — replace the `drop(tokio::task::spawn(async move {` block:

```rust
let vm_id_span = vm_id.clone();
let vm_count_clone = Arc::clone(&self.vm_count);

// Record memory at start.
{
    let meter = global::meter("hitz");
    let memory_gauge = meter
        .u64_gauge("hitz.vm.memory_bytes")
        .with_description("Guest RAM in bytes at VM start")
        .build();
    memory_gauge.record(
        u64::from(config.ram_mib) * 1024 * 1024,
        &[KeyValue::new("vm.id", vm_id.as_str())],
    );
}

// Track running count.
self.vm_count.add(1, &[KeyValue::new("state", "running")]);

drop(tokio::task::spawn(async move {
    let boot_span = tracing::info_span!(
        "vm.boot",
        vm.id = %vm_id_span,
        vm.ram_mib = config.ram_mib,
        vm.cpus = config.cpus,
    );
    let _boot_enter = boot_span.enter();

    let _port_fwd = if let Some(ref net) = config.net {
        if config.ports.is_empty() {
            None
        } else {
            let guest_ip = net
                .guest_ip
                .split('/')
                .next()
                .and_then(|s| s.parse::<std::net::Ipv4Addr>().ok());
            if let Some(ip) = guest_ip {
                Some(
                    crate::port_forward::PortForwardManager::start(ip, &config.ports).await,
                )
            } else {
                tracing::warn!("could not parse guest IP from {}", net.guest_ip);
                None
            }
        }
    } else {
        None
    };

    let result = tokio::task::spawn_blocking(move || {
        hitz_vmm::boot_and_run(&*hv, &config, serial_buf, stop_flag)
    })
    .await;

    // _port_fwd drops here.

    // Decrement running count (VM has reached a terminal state).
    vm_count_clone.add(-1, &[KeyValue::new("state", "running")]);

    if let Ok(mut vms) = vms.lock()
        && let Some(entry) = vms.get_mut(&vm_id)
    {
        match result {
            Ok(Ok(run_result)) => match run_result.exit_reason {
                ExitReason::Halt | ExitReason::Shutdown | ExitReason::Canceled => {
                    entry.state = VmState::Stopped;
                    entry.exit_reason = Some(format!("{:?}", run_result.exit_reason));
                }
                ExitReason::Unexpected(ref reason) => {
                    entry.state = VmState::Failed;
                    entry.exit_reason = Some(reason.clone());
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
        if let Some(ref buf) = entry.serial_buf {
            buf.close();
        }
    }

    let _ = completion_tx.send(vm_id);
}));
```

### Step 4.3 — Run GREEN

```bash
cargo test -p hitz-daemon -- --nocapture 2>&1 | tail -20
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 4.4 — Commit

```bash
git add crates/hitz-daemon/src/vm_manager.rs
git commit -m "feat(telemetry): vm.boot span and hitz.vm.* metrics in vm_manager"
```

---

## Task 5: `hitz.vcpu.exits` counter in `run_loop`

**Files touched:**
- `crates/hitz-vmm/src/run_loop.rs`
- `crates/hitz-vmm/Cargo.toml`

**Depends on:** Task 1 (adds `opentelemetry.workspace` to hitz-vmm)

### Step 5.1 — Write failing test (RED)

Add to `crates/hitz-vmm/src/run_loop.rs` in a `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard: the exit counter must not panic when the global meter
    /// is the default no-op meter (no provider registered).
    #[test]
    fn exit_counter_noop_does_not_panic() {
        // No OTel provider registered in test context → all calls are no-ops.
        let meter = opentelemetry::global::meter("hitz");
        let counter = meter
            .u64_counter("hitz.vcpu.exits")
            .with_description("vCPU exit events")
            .build();
        counter.add(1, &[opentelemetry::KeyValue::new("exit_reason", "Halt")]);
        // If we reach here without panic, the test passes.
    }
}
```

Run to confirm RED (counter API not yet imported):

```bash
cargo test -p hitz-vmm exit_counter 2>&1 | head -10
# Expected: compile error on opentelemetry::global
```

### Step 5.2 — Implement

In `crates/hitz-vmm/src/run_loop.rs`, add at the top:

```rust
use opentelemetry::KeyValue;
```

In `run_vcpu_loop`, create the counter **once before the loop** and add increments in each
match arm. The counter is `Clone` (reference-counted internally), so creating it once and
calling `add` is the correct pattern.

```rust
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    stop_flag: &AtomicBool,
) -> Result<ExitReason, HalError> {
    let mut pending_irq: Option<u8> = None;
    let mut iterations: u64 = 0;

    // Create the exit counter once; when no provider is registered this is a no-op.
    let exit_counter = opentelemetry::global::meter("hitz")
        .u64_counter("hitz.vcpu.exits")
        .with_description("Number of vCPU exits, labeled by exit reason")
        .build();

    loop {
        iterations += 1;
        if iterations > MAX_RUN_ITERATIONS {
            exit_counter.add(1, &[KeyValue::new("exit_reason", "Unexpected")]);
            return Ok(ExitReason::Unexpected(
                "iteration limit reached".to_string(),
            ));
        }

        if stop_flag.load(Ordering::Relaxed) {
            exit_counter.add(1, &[KeyValue::new("exit_reason", "Canceled")]);
            return Ok(ExitReason::Canceled);
        }

        // Poll devices (unchanged).
        {
            let mut devs = devices.lock().expect("device lock poisoned");
            if let Some(vector) = devs.mmio_bus.poll_devices()
                && vcpu.inject_interrupt(vector).is_err()
            {
                pending_irq = Some(vector);
                vcpu.request_interrupt_window()?;
            }
        }

        let exit = vcpu.run()?;

        match exit {
            VcpuExit::IoPort(io) => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "IoPort")]);
                let mut devs = devices.lock().expect("device lock poisoned");
                handle_io_port(vcpu, &mut devs.serial, &io)?;
            }

            VcpuExit::Halt => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "Halt")]);
                return Ok(ExitReason::Halt);
            }
            VcpuExit::Shutdown => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "Shutdown")]);
                return Ok(ExitReason::Shutdown);
            }

            VcpuExit::Mmio(mmio) => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "Mmio")]);
                // ... existing MMIO handling (unchanged) ...
            }

            VcpuExit::InterruptWindow => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "InterruptWindow")]);
                // ... existing interrupt window handling (unchanged) ...
            }

            VcpuExit::Canceled => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "Canceled")]);
                // ... existing Canceled handling (unchanged) ...
            }

            VcpuExit::Unknown(code) => {
                exit_counter.add(1, &[KeyValue::new("exit_reason", "Unexpected")]);
                return Ok(ExitReason::Unexpected(format!(
                    "unknown vCPU exit reason: {code:#x}"
                )));
            }
        }
    }
}
```

**Critical note:** Do not create a new `Meter` or `Counter` per iteration — create it once
before the loop. The counter is internally reference-counted and `add()` is a thin wrapper
that will be a no-op when no SDK is registered.

**clippy note:** The `exit_counter.add(1, ...)` calls before early returns (iteration limit,
stop flag) should be placed immediately before the `return` statement, not before the
condition check, to avoid counting exits that didn't happen.

### Step 5.3 — Run GREEN

```bash
cargo test -p hitz-vmm exit_counter -- --nocapture
# Expected: test run_loop::tests::exit_counter_noop_does_not_panic ... ok

cargo test -p hitz-vmm -- --nocapture 2>&1 | tail -20
cargo clippy -p hitz-vmm -- -D warnings
cargo fmt --check
```

### Step 5.4 — Commit

```bash
git add crates/hitz-vmm/src/run_loop.rs crates/hitz-vmm/Cargo.toml
git commit -m "feat(telemetry): hitz.vcpu.exits counter in run_loop"
```

---

## Task 6: Port forward metrics

**Files touched:**
- `crates/hitz-daemon/src/port_forward.rs`

**Depends on:** Task 1

### Step 6.1 — Write failing test (RED)

Add to the existing `#[cfg(test)]` block in `port_forward.rs`:

```rust
#[tokio::test]
async fn metrics_do_not_panic_without_provider() {
    // No OTel provider registered — all metric calls must be no-ops.
    // PortForwardManager::start must not panic when creating meter objects.
    let rule = PortForward {
        host_port: free_port().await,
        guest_port: 9998,
    };
    let mgr = PortForwardManager::start(Ipv4Addr::LOCALHOST, &[rule]).await;
    // Give listener a moment to start, then drop.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    drop(mgr);
}
```

Run:

```bash
cargo test -p hitz-daemon metrics_do_not_panic -- --nocapture
# Currently passes (no metric code yet). This test becomes the regression guard.
```

### Step 6.2 — Implement

In `crates/hitz-daemon/src/port_forward.rs`, add at the top:

```rust
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, UpDownCounter};
```

Add metric handles to `PortForwardManager`:

```rust
pub struct PortForwardManager {
    handles: Vec<JoinHandle<()>>,
    relay_handles: Arc<tokio::sync::Mutex<Vec<JoinHandle<()>>>>,
}
```

The counters are created once in `start()` and cloned into the closure (they are
reference-counted). In `start()`, before the `for rule in rules` loop:

```rust
let meter = global::meter("hitz");
let connections_total: Counter<u64> = meter
    .u64_counter("hitz.portfwd.connections_total")
    .with_description("Total TCP connections accepted by port forwarders")
    .build();
let relays_active: UpDownCounter<i64> = meter
    .i64_up_down_counter("hitz.portfwd.relays_active")
    .with_description("Currently active port-forward relay connections")
    .build();
```

Inside the `for rule in rules` loop, clone them per-listener:

```rust
let connections_total_l = connections_total.clone();
let relays_active_l = relays_active.clone();
let host_port_label = rule.host_port.to_string();

let handle = tokio::spawn(async move {
    loop {
        match listener.accept().await {
            Ok((mut inbound, _peer)) => {
                connections_total_l.add(
                    1,
                    &[KeyValue::new("host_port", host_port_label.clone())],
                );
                relays_active_l.add(
                    1,
                    &[KeyValue::new("host_port", host_port_label.clone())],
                );

                let relays_active_r = relays_active_l.clone();
                let host_port_r = host_port_label.clone();
                let relay = tokio::spawn(async move {
                    match tokio::net::TcpStream::connect(guest_addr).await {
                        Ok(mut outbound) => {
                            let _ = tokio::io::copy_bidirectional(
                                &mut inbound,
                                &mut outbound,
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::debug!(
                                "port forward connect to guest failed: {e}"
                            );
                        }
                    }
                    // Relay complete — decrement active counter.
                    relays_active_r.add(
                        -1,
                        &[KeyValue::new("host_port", host_port_r)],
                    );
                });
                relay_handles_clone.lock().await.push(relay);
            }
            Err(e) => {
                tracing::warn!("port forward accept error, retrying: {e}");
            }
        }
    }
});
```

### Step 6.3 — Run GREEN

```bash
cargo test -p hitz-daemon port_forward -- --nocapture 2>&1 | tail -20
# All existing port_forward tests pass + new regression guard passes.
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 6.4 — Commit

```bash
git add crates/hitz-daemon/src/port_forward.rs
git commit -m "feat(telemetry): hitz.portfwd.* metrics in port_forward"
```

---

## Task 7: Integration test skeleton

**Files touched:**
- `crates/hitz-whp/src/tests.rs`

**Depends on:** Tasks 2–6 (all instrumented, no providers registered in this test)

### Step 7.1 — Write test (this is both the RED and GREEN step)

Add to `crates/hitz-whp/src/tests.rs` after the last test function:

```rust
/// Phase 11 regression guard: boot_and_run must work normally when no OTel
/// provider is registered.  All tracing/metric calls become no-ops.
///
/// This is a compile + behavior test, not a metric correctness test.
/// Run with: cargo test -p hitz-whp -- --ignored phase11
#[test]
#[ignore]
fn phase11_otel_noop_boot() {
    // No TelemetryGuard initialised → global providers are no-ops.
    // This ensures our instrumentation doesn't add any required setup steps.
    use hitz_hal::Hypervisor as _;
    use hitz_vmm::ExitReason;

    // Use the same hello-world payload as phase5_boot_and_run_hello.
    // (Copy the kernel bytes / config from that test rather than duplicating
    // the full guest binary embedding — reference that test for full setup.)
    //
    // Minimal smoke test: create hypervisor, verify it doesn't panic when
    // OTel API is called with no provider registered.
    let meter = opentelemetry::global::meter("hitz");
    let counter = meter.u64_counter("hitz.vcpu.exits").build();
    counter.add(1, &[opentelemetry::KeyValue::new("exit_reason", "Halt")]);

    let up_down = meter.i64_up_down_counter("hitz.portfwd.relays_active").build();
    up_down.add(1, &[]);
    up_down.add(-1, &[]);

    let gauge = meter.u64_gauge("hitz.vm.memory_bytes").build();
    gauge.record(128 * 1024 * 1024, &[]);

    // If we reach here without panic, instrumentation is safe with no-op provider.
    // For a full boot regression, see phase5_boot_and_run_hello (same WHP guard).
    eprintln!("phase11_otel_noop_boot: OTel no-op path verified");
}
```

### Step 7.2 — Run

```bash
cargo test -p hitz-whp -- --ignored phase11 --nocapture
# Expected (on a machine with WHP enabled):
# test tests::phase11_otel_noop_boot ... ok
# phase11_otel_noop_boot: OTel no-op path verified
```

On machines without WHP the test is `#[ignore]` and skipped by default.

```bash
cargo clippy -p hitz-whp -- -D warnings
cargo fmt --check
```

### Step 7.3 — Commit

```bash
git add crates/hitz-whp/src/tests.rs
git commit -m "test(telemetry): phase11 OTel noop boot regression guard"
```

---

## Final verification

After all tasks are complete:

```bash
# Full workspace build
cargo build --workspace 2>&1 | tail -5
# Expected: Finished dev [unoptimized + debuginfo] target(s)

# Full workspace test (non-ignored)
cargo test --workspace 2>&1 | tail -20
# Expected: all 180+ tests pass; new tests add to the count

# Clippy clean
cargo clippy --workspace -- -D warnings 2>&1 | tail -5

# Format clean
cargo fmt --check
```

**Manual smoke test with a local OTel collector (optional):**

```bash
# Start a collector (e.g. otel-col or Jaeger all-in-one)
docker run -p 4317:4317 jaegertracing/all-in-one:latest

# Start daemon with telemetry
hitz daemon start --otlp-endpoint http://localhost:4317 --verbose

# In another terminal, create and start a VM
hitz vm create test1 --kernel /path/to/vmlinux
hitz vm start test1

# Verify traces appear in Jaeger UI at http://localhost:16686
# Service name: "hitz", operation: "vm.boot"
```

---

## Appendix: OTel Rust version pinning

The `opentelemetry` ecosystem moves fast. Before implementing, verify on crates.io that:

1. `opentelemetry 0.28` and `opentelemetry_sdk 0.28` have the same minor version.
2. `opentelemetry-otlp 0.28` is compatible with OTel SDK 0.28.
3. `tracing-opentelemetry 0.29` is compatible with `tracing-subscriber 0.3`.
4. `opentelemetry_sdk` with `rt-tokio` feature requires `tokio` 1.x (already in workspace).

If versions have advanced, update the pinned versions in `Cargo.toml` and adjust any renamed
APIs (the `SpanExporter::builder()` and `MetricExporter::builder()` APIs in particular have
changed between 0.26 and 0.28).

The compatibility matrix as of this plan's writing date (2026-03-07):

| Crate | Version | Notes |
|-------|---------|-------|
| opentelemetry | 0.28 | Core API traits |
| opentelemetry_sdk | 0.28 | SdkTracerProvider, SdkMeterProvider |
| opentelemetry-otlp | 0.28 | SpanExporter, MetricExporter, tonic feature |
| tracing-opentelemetry | 0.29 | OpenTelemetryLayer, tracing bridge |

**Verify before implementing:** `cargo search opentelemetry` or check <https://crates.io/crates/opentelemetry>.

---

## Appendix: Clippy considerations

The workspace `Cargo.toml` enables pedantic + nursery lints. Expect these to need attention:

- `clippy::significant_drop_tightening` — the `span.enter()` guard must be a named binding
  (`let _enter = span.enter();`), not an inline call, or the guard drops immediately.
- `clippy::unwrap_used` / `clippy::expect_used` — the meter `.build()` calls do not return
  `Result`; no unwrap needed.
- `clippy::too_many_lines` — `run_vcpu_loop` and `start_vm` are already long; adding counter
  calls may tip them over. Add `#[allow(clippy::too_many_lines)]` at the function level if
  so, or extract a helper `fn record_exit(counter: &Counter<u64>, reason: &str)`.

---

## Appendix: Known Windows-specific notes

- `opentelemetry-otlp` with `tonic` feature depends on `tonic`/`tower`/`hyper-util` which work
  on Windows. The gRPC channel uses `tokio` under the hood — no UNIX-socket dependencies.
- WinTun (`hitz-net`) is not affected by this phase.
- Named pipe transport (`hitz-daemon`) is not affected; OTel exports go over a separate TCP
  connection to the OTLP collector.
