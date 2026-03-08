# Phase 11: Observability — Design

## Goal

Give production engineers full visibility into Hitz daemon and VM internals via OpenTelemetry traces and metrics exported over gRPC OTLP to any compatible backend (Jaeger, Grafana Tempo, Honeycomb, Datadog, etc.).

## Scope

**In:** Host-side traces (boot latency, API request latency), host-side metrics (vCPU exits, memory, port forward connections, VM state counts), OTLP gRPC export, `tracing` integration, structured lifecycle events.

**Out:** Guest-side metrics (CPU%, memory inside VM — Phase 12), Prometheus `/metrics` scrape endpoint, `hitz run` standalone telemetry, dynamic enable/disable without restart.

## Architecture

```
hitz daemon start [--otlp-endpoint http://localhost:4317]
    └─ telemetry::init(endpoint) → TelemetryGuard
         ├─ SdkTracerProvider (OTLP gRPC) ──► Jaeger / Tempo / ...
         └─ SdkMeterProvider  (OTLP gRPC) ──► Grafana / Datadog / ...

tracing spans ──► tracing-opentelemetry layer ──► SdkTracerProvider
OTel metrics  ──► opentelemetry::global::meter() ──► SdkMeterProvider
```

When no endpoint is configured the pipeline is a no-op — zero cost, no background threads, no panics. Inner crates (`hitz-vmm`, `hitz-hal`, etc.) stay dep-free; they use `tracing::info!` and `opentelemetry::global::meter()` which are no-ops without an initialized pipeline.

## New Module: `hitz-daemon/src/telemetry.rs`

```rust
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let _ = self.tracer_provider.shutdown();
        let _ = self.meter_provider.shutdown();
    }
}

/// Initialize the OpenTelemetry pipeline.
///
/// Returns a no-op guard when `endpoint` is `None`.
/// Endpoint resolution order: `endpoint` arg → `OTEL_EXPORTER_OTLP_ENDPOINT` env var → no-op.
pub fn init(endpoint: Option<&str>) -> Result<TelemetryGuard, Box<dyn std::error::Error>>;
```

The guard is created in `run_daemon` right after the tokio runtime, and dropped last — after `stop_all_and_wait` and the server handle — so every in-flight span closes before the exporter shuts down.

## CLI

`DaemonStartArgs` gains:

```rust
/// OTLP gRPC endpoint (e.g. http://localhost:4317).
/// Falls back to OTEL_EXPORTER_OTLP_ENDPOINT env var. Omit to disable.
#[arg(long)]
otlp_endpoint: Option<String>,
```

Endpoint resolution in `run_daemon`:

```rust
let endpoint = args.otlp_endpoint.as_deref().or_else(|| {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok().as_deref()
});
let _telemetry = telemetry::init(endpoint)?;
```

## Instrumentation Points

### Traces (spans)

| Span name | Location | Key attributes |
|-----------|----------|----------------|
| `vm.boot` | `hitz-daemon/src/vm_manager.rs` | `vm.id`, `vm.ram_mib`, `vm.cpus`, `vm.has_disk`, `vm.has_net` |
| `daemon.request` | `hitz-daemon/src/router.rs` | `http.method`, `http.route`, `http.status_code` |

`vm.boot` starts when `start_vm` fires the spawned task and ends when the first byte is written to `SerialBuf`. Uses `tracing::info_span!("vm.boot", vm.id = %vm_id)` — bridged automatically to OTel via `tracing-opentelemetry`.

### Metrics (counters / gauges / histograms)

| Metric | Type | Location | Labels |
|--------|------|----------|--------|
| `hitz.vcpu.exits` | Counter | `hitz-vmm/src/run_loop.rs` | `exit_reason` |
| `hitz.vm.memory_bytes` | Gauge | `hitz-daemon/src/vm_manager.rs` | `vm.id` |
| `hitz.portfwd.connections_total` | Counter | `hitz-daemon/src/port_forward.rs` | `vm.id`, `host_port` |
| `hitz.portfwd.relays_active` | UpDownCounter | `hitz-daemon/src/port_forward.rs` | `vm.id` |
| `hitz.vm.count` | UpDownCounter | `hitz-daemon/src/vm_manager.rs` | `state` |

`hitz.vcpu.exits` is the hottest path — it increments every vCPU exit. With a no-op meter this is a single atomic load (the no-op check) per exit, acceptable overhead.

### Structured Log Events

Consistent `tracing` fields on existing lifecycle events:

- `vm.created` → `tracing::info!(vm.id, vm.ram_mib, vm.cpus)`
- `vm.started` → `tracing::info!(vm.id)`
- `vm.stopped` → `tracing::info!(vm.id, exit_reason)`

## Dependencies

Added to workspace `Cargo.toml`:

```toml
opentelemetry         = "0.28"
opentelemetry_sdk     = { version = "0.28", features = ["rt-tokio"] }
opentelemetry-otlp    = { version = "0.28", features = ["tonic"] }
tracing-opentelemetry = "0.29"
```

Only `hitz-daemon` gains these as direct deps. `hitz-vmm` gains `opentelemetry` (for `global::meter()`) only if adding the vcpu exit counter there; otherwise the counter can live in the run-loop caller inside `hitz-daemon` by passing a counter handle in.

## Shutdown Sequence

```
ctrl_c
  └─ shutdown_tx.send(true)         // stop listeners
  └─ stop_all_and_wait(5s)          // drain VMs → spans close
  └─ server_handle.await            // server exits
  └─ _telemetry drops               // flush OTLP, shutdown providers
```

## Files Changed

| File | Change |
|------|--------|
| `hitz-daemon/src/telemetry.rs` | NEW: `TelemetryGuard`, `init()` |
| `hitz-daemon/src/lib.rs` | `pub mod telemetry` |
| `hitz-daemon/src/vm_manager.rs` | `vm.boot` span, `hitz.vm.*` metrics |
| `hitz-daemon/src/port_forward.rs` | connection counter, relay gauge |
| `hitz-daemon/src/router.rs` | `daemon.request` span per handler |
| `hitz-vmm/src/run_loop.rs` | `hitz.vcpu.exits` counter |
| `hitz-cli/src/main.rs` | `--otlp-endpoint` flag, `init()` call |
| `Cargo.toml` (workspace) | new OTEL deps |

## Testing

**Unit (no WHP, no OTLP backend):**
- `telemetry::init(None)` → no-op guard, drops cleanly
- `telemetry::init(Some("http://localhost:19999"))` → init succeeds (OTLP is lazy), drops cleanly with flush attempt
- Endpoint resolution: CLI flag overrides env var; env var used when flag absent; `None` when neither set
- `hitz.vcpu.exits` counter increments on each `ExitReason` variant (unit test with mock meter)

**Integration (`#[ignore]`, WHP + networking):**
- `phase11_otlp_traces`: boot VM with OTLP endpoint pointing at a local collector stub, assert `vm.boot` span received with duration > 0 and correct `vm.id` attribute
