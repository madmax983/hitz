//! Hitz CLI — micro-VM manager for Windows.
//!
//! Usage:
//!   `hitz run --kernel vmlinux [--initramfs init.cpio] [-v]`
//!   `hitz daemon start [--pipe \\.\pipe\hitz]`
//!   `hitz vm create <ID> --kernel vmlinux [--initramfs ...]`
//!   `hitz vm start <ID>`

// CLI binary — anyhow for top-level errors, expect on infallible ops.
#![allow(clippy::expect_used)]

mod analyzer;
mod pipe_client;

use std::ffi::OsString;
use std::io::{BufWriter, Write, stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use hitz_api::{
    ActionVmRequest, CloneVmRequest, CreateVmRequest, DEFAULT_CMDLINE, DEFAULT_CPUS,
    DEFAULT_GUEST_CID, DEFAULT_RAM_MIB, GuestAgentMode, VmAction, VmConfig,
};
use hitz_daemon::TelemetryGuard;
use hitz_vmm::ExitReason;
use hitz_whp::WhpHypervisor;
use hyper::Method;

/// Default named pipe path for the daemon.
const DEFAULT_PIPE: &str = r"\\.\pipe\hitz";

/// Windows service name registered with the SCM.
const SERVICE_NAME: &str = "hitz";

/// Parse a `HOST:GUEST` port forward string into a [`hitz_api::PortForward`].
fn parse_port_forward(s: &str) -> Result<hitz_api::PortForward, String> {
    let (host, guest) = s
        .split_once(':')
        .ok_or_else(|| format!("expected HOST:GUEST (e.g. 2222:22), got {s:?}"))?;
    Ok(hitz_api::PortForward {
        host_port: host
            .parse()
            .map_err(|_| format!("invalid host port {host:?}: must be 0-65535"))?,
        guest_port: guest
            .parse()
            .map_err(|_| format!("invalid guest port {guest:?}: must be 0-65535"))?,
    })
}

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

    /// Number of virtual CPUs.
    #[arg(long, default_value_t = DEFAULT_CPUS)]
    cpus: u32,

    /// Kernel command line.
    #[arg(long, default_value = DEFAULT_CMDLINE)]
    cmdline: String,

    /// Verbose output (banner + exit summary on stderr).
    #[arg(short, long)]
    verbose: bool,

    /// Enable networking.
    #[arg(long)]
    net: bool,

    /// Host-side IP address with prefix.
    #[arg(long, default_value = hitz_api::DEFAULT_HOST_IP)]
    host_ip: String,

    /// Guest-side IP address with prefix.
    #[arg(long, default_value = hitz_api::DEFAULT_GUEST_IP)]
    guest_ip: String,

    /// Guest MAC address.
    #[arg(long)]
    mac: Option<String>,

    /// TCP port forwards HOST:GUEST (e.g. `--port 2222:22`). Repeatable.
    #[arg(long = "port", value_name = "HOST:GUEST", value_parser = parse_port_forward)]
    ports: Vec<hitz_api::PortForward>,
}

// ── Daemon ──

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

/// Arguments for `daemon install`.
#[derive(Parser)]
struct DaemonInstallArgs {
    /// Runtime arguments forwarded verbatim to `daemon start` at service launch.
    #[command(flatten)]
    start: DaemonStartArgs,

    /// Auto-start at boot (`ServiceStartType::AutoStart`). Default: manual start.
    #[arg(long)]
    auto_start: bool,

    /// Display name shown in Services MMC. Default: "Hitz micro-VM daemon".
    #[arg(long)]
    display_name: Option<String>,

    /// Service description. Default: "Hyper-V microVM manager".
    #[arg(long)]
    description: Option<String>,
}

/// Build the `launch_arguments` vec baked into the SCM registry entry.
///
/// The SCM invokes: `hitz.exe daemon start <these args>`.
/// `--pipe` and `--state-dir` are always explicit so the service does not
/// depend on runtime defaults or `%APPDATA%` being set for the `LocalSystem`
/// account.
fn build_launch_args(args: &DaemonStartArgs) -> Result<Vec<String>> {
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
    // Always resolve and include --state-dir at install time so the service
    // does not depend on %APPDATA% being set for the LocalSystem account.
    let state_dir = args
        .state_dir
        .clone()
        .unwrap_or_else(|| resolve_state_dir(None));
    let state_dir_str = state_dir
        .to_str()
        .context("state-dir path is not valid UTF-8")?
        .to_owned();
    v.push("--state-dir".to_string());
    v.push(state_dir_str);
    Ok(v)
}

fn install_service(args: &DaemonInstallArgs) -> Result<()> {
    use windows_service::{
        service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let exe = std::env::current_exe().context("failed to get current exe path")?;

    let launch_arguments: Vec<OsString> = build_launch_args(&args.start)?
        .into_iter()
        .map(OsString::from)
        .collect();

    let display_name = args
        .display_name
        .clone()
        .unwrap_or_else(|| "Hitz micro-VM daemon".to_string());
    let description = args
        .description
        .clone()
        .unwrap_or_else(|| "Hyper-V micro-VM manager".to_string());

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
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(display_name),
        service_type: ServiceType::OWN_PROCESS,
        start_type,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments,
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service = manager
        .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
        .context("failed to create service — service may already exist, or access denied (run as Administrator)")?;

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

fn remove_service() -> Result<()> {
    use windows_service::{
        service::ServiceAccess,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("failed to open SCM — run as Administrator")?;

    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::DELETE | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
        )
        .context("service 'hitz' not found — is it installed?")?;

    // Best-effort stop before delete (service may already be stopped).
    let _ = service.stop();

    service.delete().context("failed to delete service")?;

    println!("removed: hitz Windows service");
    Ok(())
}

/// Arguments for `daemon start`.
#[derive(Args)]
struct DaemonStartArgs {
    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Optional TCP address to listen on (e.g. 127.0.0.1:8080).
    #[arg(long)]
    tcp_listen: Option<std::net::SocketAddr>,

    /// Verbose output.
    #[arg(short, long)]
    verbose: bool,

    /// OTLP gRPC collector endpoint (e.g. `<http://localhost:4317>`).
    /// Falls back to `OTEL_EXPORTER_OTLP_ENDPOINT` env var. Omit to disable telemetry.
    #[arg(long)]
    otlp_endpoint: Option<String>,

    /// Directory for persisted VM state.
    /// Default: `%APPDATA%\hitz\vms` (Windows) or `.hitz/vms` (fallback).
    #[arg(long)]
    state_dir: Option<PathBuf>,
}

// ── VM ──

/// VM lifecycle subcommands (requires running daemon).
#[derive(Subcommand)]
enum VmCommand {
    /// Create a VM with the given configuration.
    Create(Box<VmCreateArgs>),
    /// Clone an existing VM.
    Clone(Box<VmCloneArgs>),
    /// Start (boot) a previously created VM.
    Start(VmIdArgs),
    /// Stop a running VM.
    Stop(VmIdArgs),
    /// Restart a running VM.
    Restart(VmIdArgs),
    /// Get VM status.
    Status(VmIdArgs),
    /// List all VMs.
    List(VmListArgs),
    /// Delete a stopped VM.
    Delete(VmIdArgs),
    /// Stream serial console output.
    Serial(VmIdArgs),
    /// Display live resource metrics for a running VM.
    Metrics(VmIdArgs),
    /// Display live interactive resource dashboard.
    Top(VmIdArgs),
    /// Live TUI dashboard for all VMs.
    Dashboard(VmListArgs),
    /// Export metrics to JSON format.
    ExportMetrics(Box<VmExportArgs>),
    /// Record metrics over time to JSON Lines format.
    Record(Box<VmRecordArgs>),
    /// Analyze VM health based on configuration and current metrics.
    Analyze(VmIdArgs),
    /// Replay metrics recording and output a timeline of health state changes.
    Timeline(VmTimelineArgs),
}

/// Arguments for `vm clone`.
#[derive(Parser)]
struct VmCloneArgs {
    /// Source VM identifier.
    src_id: String,

    /// Destination VM identifier.
    dest_id: String,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,
}

/// Arguments for `vm record`.
#[derive(Parser)]
struct VmRecordArgs {
    /// VM identifier.
    id: String,

    /// Path to output the metrics stream (JSON Lines format).
    #[arg(short, long)]
    out: PathBuf,

    /// Polling interval in milliseconds.
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,

    /// Duration to record for in seconds (if not specified, records until Ctrl-C).
    #[arg(long)]
    duration_secs: Option<u64>,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,
}

/// Arguments for `vm timeline`.
#[derive(Parser)]
struct VmTimelineArgs {
    /// Path to the recorded metrics stream (JSON Lines format).
    #[arg(short, long)]
    in_file: PathBuf,
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

    /// Number of virtual CPUs.
    #[arg(long, default_value_t = DEFAULT_CPUS)]
    cpus: u32,

    /// Kernel command line.
    #[arg(long, default_value = DEFAULT_CMDLINE)]
    cmdline: String,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,

    /// Enable networking.
    #[arg(long)]
    net: bool,

    /// Host-side IP address with prefix.
    #[arg(long, default_value = hitz_api::DEFAULT_HOST_IP)]
    host_ip: String,

    /// Guest-side IP address with prefix.
    #[arg(long, default_value = hitz_api::DEFAULT_GUEST_IP)]
    guest_ip: String,

    /// Guest MAC address.
    #[arg(long)]
    mac: Option<String>,

    /// TCP port forwards HOST:GUEST (e.g. `--port 2222:22`). Repeatable.
    #[arg(long = "port", value_name = "HOST:GUEST", value_parser = parse_port_forward)]
    ports: Vec<hitz_api::PortForward>,
}

/// Arguments for `vm export-metrics`.
#[derive(Parser)]
struct VmExportArgs {
    /// VM identifier.
    id: String,

    /// Path to export the metrics to (JSON).
    #[arg(short, long)]
    out: PathBuf,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,
}

/// Arguments that take just a VM ID.
#[derive(Parser)]
struct VmIdArgs {
    /// VM identifier.
    id: String,

    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,
}

/// Arguments for `vm list`.
#[derive(Parser)]
struct VmListArgs {
    /// Named pipe path.
    #[arg(long, default_value = DEFAULT_PIPE)]
    pipe: String,

    /// Connect to daemon via TCP instead of named pipe.
    #[arg(long)]
    tcp: Option<std::net::SocketAddr>,
}

// ── Helpers ──

/// Resolve the OTLP endpoint from the CLI flag, falling back to the
/// `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.
///
/// Returns `None` when neither is set, which disables telemetry.
fn resolve_otlp_endpoint(cli_arg: Option<String>) -> Option<String> {
    cli_arg.or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
}

/// Resolve the state directory from the CLI flag, falling back to
/// `%APPDATA%\hitz\vms` on Windows.
fn resolve_state_dir(cli_arg: Option<PathBuf>) -> PathBuf {
    cli_arg.unwrap_or_else(|| {
        std::env::var("APPDATA").map_or_else(
            |_| {
                let fallback = PathBuf::from(".").join("hitz").join("vms");
                eprintln!(
                    "hitz: warning: APPDATA not set, state dir defaults to {}",
                    fallback.display()
                );
                fallback
            },
            |appdata| PathBuf::from(appdata).join("hitz").join("vms"),
        )
    })
}

/// Initialise the tracing subscriber with an optional fmt layer and optional `OTel` layer.
///
/// Uses `try_init` so a pre-existing subscriber (e.g. in unit tests) is not replaced.
fn init_tracing(telemetry: &TelemetryGuard, verbose: bool) {
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::prelude::*;

    let mut layers: Vec<
        Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
    > = Vec::new();

    if verbose {
        let fmt = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(tracing_subscriber::filter::EnvFilter::new("hitz=debug"))
            .boxed();
        layers.push(fmt);
    }

    if let Some(otel) = telemetry.tracing_layer() {
        layers.push(otel.boxed());
    }

    let _ = tracing_subscriber::registry().with(layers).try_init();
}

// Generate the FFI service entry point for the Windows Service Control Manager.
windows_service::define_windows_service!(ffi_service_main, service_main);

/// Called by the SCM dispatcher on a dedicated thread when the service starts.
///
/// Parses the `arguments` (baked into the service registry binary path at install
/// time), registers the stop control handler, reports service state transitions,
/// and runs the shared daemon body via [`run_daemon_inner`].
// Called only from the FFI entry point generated by define_windows_service!
// Vec<OsString> is required by the windows-service macro contract.
#[allow(dead_code, clippy::needless_pass_by_value)]
fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service(&arguments) {
        eprintln!("hitz service error: {e:#}");
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

// Called only from service_main (itself FFI-only); suppress dead_code + pass-by-value.
#[allow(dead_code, clippy::too_many_lines)]
fn run_service(arguments: &[OsString]) -> anyhow::Result<()> {
    use std::time::Duration;
    use windows_service::{
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
    };

    // tokio::sync::watch::Sender::send is synchronous (atomic write), safe to call
    // from the SCM control handler callback without any async machinery.
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);

    let event_handler = move |control: ServiceControl| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop_tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .context("failed to register service control handler")?;

    // Report: starting up.
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
        .context("failed to report StartPending")?;

    // Re-parse the arguments baked in at install time.
    // SCM passes arguments as: [exe_path, "daemon", "start", "--pipe", ...]
    // We need to find where "daemon" starts and prepend "hitz" as argv[0].
    //
    // The first element from SCM may be the exe path or empty; we skip any
    // element that looks like a file path (contains a path separator).
    let daemon_args_start = arguments
        .iter()
        .position(|a| a.to_string_lossy() == "daemon")
        .unwrap_or(1); // fallback: skip only the exe path element
    let mut argv: Vec<OsString> = vec![OsString::from("hitz")];
    argv.extend_from_slice(&arguments[daemon_args_start..]);

    let cli = Cli::try_parse_from(argv).context(
        "failed to parse service arguments — was the service registered with 'daemon start'?",
    )?;

    let Command::Daemon(DaemonCommand::Start(daemon_args)) = cli.command else {
        anyhow::bail!(
            "service binary path must start with 'daemon start'; got unexpected subcommand"
        );
    };

    // Report: running.
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
        .context("failed to report Running")?;

    // Init telemetry (same as foreground path in run_daemon).
    let endpoint = resolve_otlp_endpoint(daemon_args.otlp_endpoint.clone());
    let telemetry = hitz_daemon::TelemetryGuard::init(endpoint);
    init_tracing(&telemetry, daemon_args.verbose);

    let rt =
        tokio::runtime::Runtime::new().context("failed to create tokio runtime in service mode")?;

    let result = rt.block_on(async move {
        // Shutdown future: resolves when SCM sends Stop/Shutdown (value = true).
        // Loop so a spurious send(false) does not trigger premature shutdown.
        let shutdown = async move {
            loop {
                if stop_rx.changed().await.is_err() {
                    break; // sender dropped — treat as stop
                }
                if *stop_rx.borrow() {
                    break;
                }
            }
        };
        run_daemon_inner(&daemon_args, shutdown).await
    });

    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);

    let exit_code = u32::from(result.is_err());

    // Report: stopped (best-effort — SCM may already have moved on).
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

// ── Main ──

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Run(args) => run_vm(args).unwrap_or_else(|e| {
            use crossterm::style::Stylize;
            eprintln!("\r\x1b[2K{}", format!("✗ Error: {e:#}").red().bold());
            ExitCode::FAILURE
        }),
        Command::Daemon(cmd) => {
            let result = match cmd {
                DaemonCommand::Start(ref args) => run_daemon(args),
                DaemonCommand::Install(ref args) => install_service(args),
                DaemonCommand::Remove => remove_service(),
            };
            result.map_or_else(
                |e| {
                    use crossterm::style::Stylize;
                    eprintln!("\r\x1b[2K{}", format!("✗ Error: {e:#}").red().bold());
                    ExitCode::FAILURE
                },
                |()| ExitCode::SUCCESS,
            )
        }
        Command::Vm(cmd) => run_vm_command(cmd).map_or_else(
            |e| {
                use crossterm::style::Stylize;
                eprintln!("\r\x1b[2K{}", format!("✗ Error: {e:#}").red().bold());
                ExitCode::FAILURE
            },
            |()| ExitCode::SUCCESS,
        ),
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
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
        eprintln!("hitz: CPUs {}", args.cpus);
        eprintln!("hitz: cmdline \"{}\"", args.cmdline);
    }

    if !args.ports.is_empty() {
        eprintln!(
            "hitz: warning: --port flags are not supported in standalone mode (use daemon: \
             `hitz daemon start` + `hitz vm create --port ...`)"
        );
    }

    let net = if args.net {
        Some(hitz_api::NetConfig {
            mac: args.mac,
            host_ip: args.host_ip,
            guest_ip: args.guest_ip,
            adapter_name: None,
        })
    } else {
        None
    };

    let config = VmConfig {
        kernel_path: args.kernel,
        initramfs_path: args.initramfs,
        disk_path: args.disk,
        ram_mib: args.ram,
        cpus: args.cpus,
        cmdline: Some(args.cmdline),
        net,
        ports: args.ports,
        guest_cid: DEFAULT_GUEST_CID,
        guest_agent: GuestAgentMode::Auto,
    };

    let hypervisor = WhpHypervisor::new().context("failed to create WHP hypervisor")?;

    // Set up Ctrl+C handler to stop the VM gracefully.
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::Relaxed);
    })
    .context("failed to set Ctrl+C handler")?;

    // BufWriter avoids per-byte syscall on stdout (serial writes one byte at a time).
    // Use `Stdout` (not `StdoutLock`) since it must be `Send` for multi-vCPU threads.
    let serial_out = BufWriter::new(stdout());

    let result = hitz_vmm::boot_and_run(
        &hypervisor,
        &config,
        serial_out,
        stop_flag,
        // No vsock in standalone mode; BootExtras::none() permanently for hitz run.
        hitz_vmm::BootExtras::none(),
    )
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
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

// ── hitz daemon start ──

/// Shared async body for foreground and service daemon modes.
///
/// `shutdown` resolves when the caller wants the daemon to stop:
/// - foreground: `tokio::signal::ctrl_c()` future
/// - service mode: a `tokio::sync::watch` receiver future (Task 4)
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
    let server_handle =
        tokio::spawn(
            async move { hitz_daemon::run_server(&pipe, tcp, server_mgr, shutdown_rx).await },
        );

    shutdown.await;
    eprintln!("\nhitz: shutting down...");

    // Signal listeners to stop accepting new connections.
    let _ = shutdown_tx.send(true);
    // Gracefully stop all running VMs (cancel + join, 5 s timeout).
    manager
        .stop_all_and_wait(std::time::Duration::from_secs(5))
        .await;
    // Wait for the server task to exit cleanly.
    let _ = server_handle.await;

    eprintln!("hitz: shutdown complete");
    Ok(())
}

/// Run the daemon in the foreground.
///
/// Starts the named pipe (and optional TCP) listener, then waits for Ctrl+C.
/// On signal: broadcasts shutdown to the listener loops, calls
/// `stop_all_and_wait` to drain running VMs (up to 5 s), then returns.
///
/// The tokio runtime is shut down *before* the [`TelemetryGuard`] is dropped
/// so the OTLP batch exporter can flush its queue while the runtime is still
/// alive.
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
    // Auto-detect: if launched by the Service Control Manager, enter service
    // mode. service_dispatcher::start() returns
    // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT (Win32 error 1063) when NOT
    // running under the SCM — that is our signal to fall through to foreground.
    {
        use windows_service::service_dispatcher;
        match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
            Ok(()) => return Ok(()),
            Err(windows_service::Error::Winapi(ref e)) if e.raw_os_error() == Some(1063) => {
                // Not running as a service — continue to foreground mode.
            }
            Err(e) => {
                return Err(anyhow::Error::new(e).context("service dispatcher error"));
            }
        }
    }

    let endpoint = resolve_otlp_endpoint(args.otlp_endpoint.clone());

    // Init OTel providers BEFORE subscriber registration so the tracer exists
    // when the layer is built.
    let telemetry = TelemetryGuard::init(endpoint);
    init_tracing(&telemetry, args.verbose);

    eprintln!("hitz: daemon listening on {}", args.pipe);
    if let Some(addr) = args.tcp_listen {
        eprintln!("hitz: TCP listener on {addr}");
    }

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    let result = rt.block_on(async {
        let shutdown = async {
            // ctrl_c() only errors if the OS signal handler cannot be registered;
            // treat that as a stop signal rather than panicking.
            tokio::signal::ctrl_c().await.unwrap_or(());
        };
        run_daemon_inner(args, shutdown).await
    });

    // Shut down the runtime BEFORE dropping the OTel guard so the batch
    // exporter can flush pending spans while the async executor is still live.
    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);

    result
}

// ── Metrics formatting ──

/// Format a [`hitz_api::MetricsSnapshot`] into a human-readable string for CLI display.
#[allow(clippy::too_many_lines)]
fn format_metrics_snapshot(snap: &hitz_api::MetricsSnapshot) -> String {
    use comfy_table::presets::UTF8_FULL_CONDENSED;
    use comfy_table::{Attribute, Cell, Color, Table};
    use std::fmt::Write as _;

    let mut out = String::new();

    // ── System ──
    let mut sys_table = Table::new();
    let _ = sys_table.load_preset(UTF8_FULL_CONDENSED);

    // Pre-allocating a single `String` buffer with estimated capacity
    // and using `write!` macro directly removes the intermediate heap
    // allocations for formatting strings into a `Vec` and then calling
    // `.join()`. This eliminates unnecessary allocations on the hot path
    // when formatting CLI output.
    let mut cores_str = String::with_capacity(snap.cpu.per_core.len() * 8);
    for (i, p) in snap.cpu.per_core.iter().enumerate() {
        if i > 0 {
            cores_str.push_str(", ");
        }
        let _ = write!(cores_str, "{p:.1}%");
    }

    let cpu_load = format!(
        "{:.2} / {:.2} / {:.2}",
        snap.cpu.load_avg[0], snap.cpu.load_avg[1], snap.cpu.load_avg[2]
    );

    let used_mib = snap.memory.used_bytes / (1024 * 1024);
    let total_mib = snap.memory.total_bytes / (1024 * 1024);
    let mem_str = format!("{used_mib} MiB / {total_mib} MiB");

    let _ = sys_table.add_row([
        Cell::new("CPU Total")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format!("{:.1}%", snap.cpu.total_pct)),
        Cell::new("Cores")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(cores_str),
    ]);
    let _ = sys_table.add_row([
        Cell::new("CPU Load")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(cpu_load),
        Cell::new("Memory")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(mem_str),
    ]);

    let _ = writeln!(out, "System:\n{sys_table}");

    // ── Disks ──
    if !snap.disks.is_empty() {
        let _ = writeln!(out);
        let mut disk_table = Table::new();
        let _ = disk_table.load_preset(UTF8_FULL_CONDENSED);
        let _ = disk_table.set_header([
            Cell::new("Disk")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Reads")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Writes")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Read KB")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Write KB")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
        ]);

        for disk in &snap.disks {
            let read_kb = disk.read_bytes / 1024;
            let write_kb = disk.write_bytes / 1024;
            let _ = disk_table.add_row([
                disk.name.clone(),
                disk.reads_total.to_string(),
                disk.writes_total.to_string(),
                read_kb.to_string(),
                write_kb.to_string(),
            ]);
        }
        let _ = writeln!(out, "Disks:\n{disk_table}");
    }

    // ── Networks ──
    if !snap.networks.is_empty() {
        let _ = writeln!(out);
        let mut net_table = Table::new();
        let _ = net_table.load_preset(UTF8_FULL_CONDENSED);
        let _ = net_table.set_header([
            Cell::new("Interface")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("RX KB")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("TX KB")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("RX Pkts")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("TX Pkts")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
        ]);

        for net in &snap.networks {
            let rx_kb = net.rx_bytes / 1024;
            let tx_kb = net.tx_bytes / 1024;
            let _ = net_table.add_row([
                net.interface.clone(),
                rx_kb.to_string(),
                tx_kb.to_string(),
                net.rx_packets.to_string(),
                net.tx_packets.to_string(),
            ]);
        }
        let _ = writeln!(out, "Networks:\n{net_table}");
    }

    // ── Processes ──
    if !snap.processes.is_empty() {
        let _ = writeln!(out);
        let mut proc_table = Table::new();
        let _ = proc_table.load_preset(UTF8_FULL_CONDENSED);
        let _ = proc_table.set_header([
            Cell::new("PID")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Name")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("CPU %")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("RSS MB")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
        ]);

        for proc in &snap.processes {
            let rss_mb = proc.rss_bytes / (1024 * 1024);
            let _ = proc_table.add_row([
                proc.pid.to_string(),
                proc.name.clone(),
                format!("{:.1}%", proc.cpu_pct),
                rss_mb.to_string(),
            ]);
        }
        let _ = writeln!(out, "Top Processes:\n{proc_table}");
    }

    out
}

// ── hitz vm * ──

fn format_error_response(status: hyper::StatusCode, resp: &str, error_prefix: &str) -> String {
    let msg = if let Ok(err) = serde_json::from_str::<hitz_api::ApiError>(resp) {
        format!("✗ {error_prefix}: {}", err.message)
    } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(resp) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            format!("✗ {error_prefix}: {}", msg)
        } else if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            format!("✗ {error_prefix}: {}", err)
        } else {
            // ⚡ Bolt Optimization: Replace intermediate `Vec` heap allocations and `.join(...)`
            // with an iterator chain using `.fold(String::new(), ...)` to eliminate
            // intermediate heap allocations and multiple `format!` calls when formatting CLI error responses.
            let parts_str = v.as_object().map_or_else(String::new, |obj| {
                obj.iter().fold(String::new(), |mut acc, (k, val)| {
                    use std::fmt::Write;
                    if let Some(s) = val.as_str() {
                        if !acc.is_empty() {
                            let _ = write!(acc, ", ");
                        }
                        let _ = write!(acc, "{k}: {s}");
                    } else if let Some(n) = val.as_number() {
                        if !acc.is_empty() {
                            let _ = write!(acc, ", ");
                        }
                        let _ = write!(acc, "{k}: {n}");
                    } else if val.is_boolean() || val.is_null() {
                        if !acc.is_empty() {
                            let _ = write!(acc, ", ");
                        }
                        let _ = write!(acc, "{k}: {val}");
                    }
                    acc
                })
            });
            if parts_str.is_empty() {
                format!("✗ {error_prefix} ({status})")
            } else {
                format!("✗ {error_prefix} ({status}): {parts_str}")
            }
        }
    } else {
        let clean = resp.trim();
        if clean.is_empty() {
            format!("✗ {error_prefix} ({status})")
        } else {
            let max_len = 200;
            let display_text = if clean.chars().count() > max_len {
                let truncated: String = clean.chars().take(max_len).collect();
                format!("{}...", truncated)
            } else {
                clean.to_string()
            };
            format!("✗ {error_prefix} ({status}): {display_text}")
        }
    };
    format!("\r\x1b[2K{}", msg)
}

fn print_error_response(status: hyper::StatusCode, resp: &str, error_prefix: &str) {
    use crossterm::style::Stylize;
    let msg = format_error_response(status, resp, error_prefix);
    println!("\r\x1b[2K{}", msg.red());
}

fn print_action_result(
    status: hyper::StatusCode,
    resp: &str,
    success_msg: &str,
    error_prefix: &str,
) {
    use crossterm::style::Stylize;
    if status.is_success() {
        println!("\r\x1b[2K{}", success_msg.green());
    } else {
        print_error_response(status, resp, error_prefix);
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

async fn handle_vm_create(args: &VmCreateArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!("{}", format!("⏳ Creating VM '{}'...", args.id).cyan());
    let _ = std::io::stdout().flush();
    let net = if args.net {
        Some(hitz_api::NetConfig {
            mac: args.mac.clone(),
            host_ip: args.host_ip.clone(),
            guest_ip: args.guest_ip.clone(),
            adapter_name: None,
        })
    } else {
        None
    };
    let config = VmConfig {
        kernel_path: args.kernel.clone(),
        initramfs_path: args.initramfs.clone(),
        disk_path: args.disk.clone(),
        ram_mib: args.ram,
        cpus: args.cpus,
        cmdline: Some(args.cmdline.clone()),
        net,
        ports: args.ports.clone(),
        guest_cid: DEFAULT_GUEST_CID,
        guest_agent: GuestAgentMode::Auto,
    };
    let body = serde_json::to_string(&CreateVmRequest { config }).context("serialize request")?;
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::PUT,
        &format!("/vms/{}", args.id),
        Some(&body),
    )
    .await?;
    print_action_result(
        status,
        &resp,
        &format!("✓ Successfully created VM {}", args.id),
        &format!("Failed to create VM {}", args.id),
    );
    Ok(())
}

async fn handle_vm_clone(args: &VmCloneArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!(
        "{}",
        format!("⏳ Cloning VM '{}' to '{}'...", args.src_id, args.dest_id).cyan()
    );
    let _ = std::io::stdout().flush();
    let body = serde_json::to_string(&CloneVmRequest {
        dest_id: args.dest_id.clone(),
    })
    .context("serialize request")?;
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::POST,
        &format!("/vms/{}/clone", args.src_id),
        Some(&body),
    )
    .await?;
    print_action_result(
        status,
        &resp,
        &format!(
            "✓ Successfully cloned VM {} to {}",
            args.src_id, args.dest_id
        ),
        &format!("Failed to clone VM {}", args.src_id),
    );
    Ok(())
}

async fn handle_vm_action(args: &VmIdArgs, action: VmAction) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    let action_str = match action {
        VmAction::Start => "Starting",
        VmAction::Restart => "Restarting",
        VmAction::Stop => "Stopping",
    };
    print!(
        "{}",
        format!("⏳ {} VM '{}'...", action_str, args.id).cyan()
    );
    let _ = std::io::stdout().flush();
    let body = serde_json::to_string(&ActionVmRequest { action }).context("serialize request")?;
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::POST,
        &format!("/vms/{}/action", args.id),
        Some(&body),
    )
    .await?;

    let (success_verb, error_verb) = match action {
        VmAction::Start => ("started", "start"),
        VmAction::Restart => ("restarted", "restart"),
        VmAction::Stop => ("stopped", "stop"),
    };

    print_action_result(
        status,
        &resp,
        &format!("✓ Successfully {} VM {}", success_verb, args.id),
        &format!("Failed to {} VM {}", error_verb, args.id),
    );
    Ok(())
}

async fn handle_vm_start(args: &VmIdArgs) -> Result<()> {
    handle_vm_action(args, VmAction::Start).await
}

async fn handle_vm_restart(args: &VmIdArgs) -> Result<()> {
    handle_vm_action(args, VmAction::Restart).await
}

async fn handle_vm_stop(args: &VmIdArgs) -> Result<()> {
    handle_vm_action(args, VmAction::Stop).await
}

#[allow(clippy::too_many_lines)]
async fn handle_vm_status(args: &VmIdArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!(
        "{}",
        format!("⏳ Fetching status for VM '{}'...", args.id).cyan()
    );
    let _ = std::io::stdout().flush();
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}", args.id),
        None,
    )
    .await?;
    if status.is_success() {
        print!("\r\x1b[2K");
        if let Ok(info) = serde_json::from_str::<hitz_api::VmInfo>(&resp) {
            use comfy_table::presets::UTF8_FULL_CONDENSED;
            use comfy_table::{Cell, Color, Table};

            let mut table = Table::new();
            let _ = table.load_preset(UTF8_FULL_CONDENSED);

            let state_cell = match info.state {
                hitz_api::VmState::Running => Cell::new("Running").fg(Color::Green),
                hitz_api::VmState::Stopped => Cell::new("Stopped").fg(Color::Yellow),
                hitz_api::VmState::Failed => Cell::new("Failed").fg(Color::Red),
                hitz_api::VmState::Created => Cell::new("Created").fg(Color::Cyan),
            };

            let _ = table.add_row([
                Cell::new("ID:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(&info.id),
            ]);
            let _ = table.add_row([
                Cell::new("State:").add_attribute(comfy_table::Attribute::Bold),
                state_cell,
            ]);
            let _ = table.add_row([
                Cell::new("Kernel:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(info.config.kernel_path.display().to_string()),
            ]);
            if let Some(ref path) = info.config.initramfs_path {
                let _ = table.add_row([
                    Cell::new("Initramfs:").add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(path.display().to_string()),
                ]);
            }
            if let Some(ref path) = info.config.disk_path {
                let _ = table.add_row([
                    Cell::new("Disk:").add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(path.display().to_string()),
                ]);
            }
            let _ = table.add_row([
                Cell::new("RAM:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(format!("{} MiB", info.config.ram_mib)),
            ]);
            let _ = table.add_row([
                Cell::new("CPUs:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(info.config.cpus.to_string()),
            ]);
            let agent_str = match info.config.guest_agent {
                hitz_api::GuestAgentMode::Auto => "Auto".to_string(),
                hitz_api::GuestAgentMode::Custom(ref p) => format!("Custom ({})", p.display()),
                hitz_api::GuestAgentMode::Disabled => "Disabled".to_string(),
            };
            let _ = table.add_row([
                Cell::new("Guest Agent:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(agent_str),
            ]);
            let _ = table.add_row([
                Cell::new("Guest CID:").add_attribute(comfy_table::Attribute::Bold),
                Cell::new(info.config.guest_cid.to_string()),
            ]);
            if let Some(ref net) = info.config.net {
                let mac_str = net.mac.as_deref().unwrap_or("Auto");
                let net_str = format!(
                    "Host IP: {}, Guest IP: {}, MAC: {}",
                    net.host_ip, net.guest_ip, mac_str
                );
                let _ = table.add_row([
                    Cell::new("Network:").add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(net_str),
                ]);
            }

            if !info.config.ports.is_empty() {
                use std::fmt::Write as _;
                // Using a pre-allocated single string buffer and directly
                // appending with the `write!` macro removes the need to allocate
                // intermediate `String` items in a `Vec` and then `join()` them
                // later, saving multiple allocations per row rendering.
                let mut ports_str = String::with_capacity(info.config.ports.len() * 24);
                for (i, p) in info.config.ports.iter().enumerate() {
                    if i > 0 {
                        ports_str.push_str(", ");
                    }
                    let _ = write!(ports_str, "0.0.0.0:{} -> {}", p.host_port, p.guest_port);
                }
                let _ = table.add_row([
                    Cell::new("Ports:").add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(ports_str),
                ]);
            }
            if let Some(reason) = &info.exit_reason {
                let _ = table.add_row([
                    Cell::new("Exit:").add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(reason),
                ]);
            }

            println!("{table}");
        } else {
            print_error_response(
                status,
                &resp,
                &format!("Failed to parse VM {} status", args.id),
            );
        }
    } else {
        print!("\r\x1b[2K");
        print_error_response(
            status,
            &resp,
            &format!("Failed to get VM {} status", args.id),
        );
    }
    Ok(())
}

async fn handle_vm_list(args: &VmListArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!("{}", "⏳ Fetching VM list...".cyan());
    let _ = std::io::stdout().flush();
    let (status, resp) =
        pipe_client::pipe_request(&args.pipe, args.tcp, Method::GET, "/vms", None).await?;
    if status.is_success() {
        print!("\r\x1b[2K");
        if let Ok(vms) = serde_json::from_str::<Vec<hitz_api::VmInfo>>(&resp) {
            if vms.is_empty() {
                use crossterm::style::Stylize;
                println!("{}", "ℹ️  No VMs found.".blue());
                return Ok(());
            }

            use comfy_table::presets::UTF8_FULL_CONDENSED;
            use comfy_table::{Cell, Color, Table};
            let mut table = Table::new();
            let _ = table.load_preset(UTF8_FULL_CONDENSED);
            let _ = table.set_header(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]);

            for info in vms {
                let state_cell = match info.state {
                    hitz_api::VmState::Running => Cell::new("Running").fg(Color::Green),
                    hitz_api::VmState::Stopped => Cell::new("Stopped").fg(Color::Yellow),
                    hitz_api::VmState::Failed => Cell::new("Failed").fg(Color::Red),
                    hitz_api::VmState::Created => Cell::new("Created").fg(Color::Cyan),
                };
                // ⚡ Bolt Optimization: Replace `.unwrap_or_else(|| "-".to_string())` with `.as_deref().unwrap_or("-")`
                // This eliminates an unnecessary `String` allocation on the hot path of formatting CLI output.
                let exit_reason = info.exit_reason.as_deref().unwrap_or("-");
                let _ = table.add_row([
                    Cell::new(&info.id),
                    state_cell,
                    Cell::new(info.config.ram_mib.to_string()),
                    Cell::new(info.config.cpus.to_string()),
                    Cell::new(exit_reason),
                ]);
            }
            println!("{table}");
        } else {
            print_error_response(status, &resp, "Failed to parse VMs list");
        }
    } else {
        print!("\r\x1b[2K");
        print_error_response(status, &resp, "Failed to list VMs");
    }
    Ok(())
}

async fn handle_vm_delete(args: &VmIdArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!("{}", format!("⏳ Deleting VM '{}'...", args.id).cyan());
    let _ = std::io::stdout().flush();
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::DELETE,
        &format!("/vms/{}", args.id),
        None,
    )
    .await?;
    print_action_result(
        status,
        &resp,
        &format!("✓ Successfully deleted VM {}", args.id),
        &format!("Failed to delete VM {}", args.id),
    );
    Ok(())
}

async fn handle_vm_serial(args: &VmIdArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!(
        "{}",
        format!("⏳ Connecting to serial console for VM '{}'...", args.id).cyan()
    );
    let _ = std::io::stdout().flush();
    use http_body_util::BodyExt as _;

    let resp =
        pipe_client::request_stream(&args.pipe, args.tcp, &format!("/vms/{}/serial", args.id))
            .await?;

    let status = resp.status();
    let mut body = resp.into_body();

    if !status.is_success() {
        let collected = body.collect().await.context("read error body")?.to_bytes();
        let text = String::from_utf8_lossy(&collected);
        anyhow::bail!("serial stream failed ({status}): {text}");
    }

    print!("\r\x1b[2K");
    let mut out = stdout();
    while let Some(frame_result) = body.frame().await {
        match frame_result {
            Ok(frame) => {
                if let Some(data) = frame.data_ref() {
                    out.write_all(data)?;
                    out.flush()?;
                }
            }
            Err(e) => {
                eprintln!("stream error: {e}");
                break;
            }
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
async fn handle_vm_top(args: &VmIdArgs) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::{
        Terminal,
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
    };
    use std::time::{Duration, Instant};

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let res = async {
        let tick_rate = Duration::from_secs(1);
        let mut last_tick = Instant::now();
        let mut last_snap: Option<hitz_api::MetricsSnapshot> = None;
        let mut last_err: Option<String> = None;

        loop {
            // Draw UI
            let _ = terminal.draw(|f| {
                let size = f.area();
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Header
                        Constraint::Length(4), // CPU & Mem Gauges
                        Constraint::Min(5),    // Main content (Tables)
                    ])
                    .split(size);

                // Header
                let header = Paragraph::new(format!("Hitz Top - VM: {}", args.id))
                    .style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(header, main_chunks[0]);

                if let Some(ref err) = last_err {
                    let err_p = Paragraph::new(err.as_str())
                        .style(Style::default().fg(Color::Red))
                        .block(Block::default().borders(Borders::ALL).title("Error"));
                    f.render_widget(err_p, main_chunks[1]);
                    return;
                }

                if let Some(ref snap) = last_snap {
                    let top_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(main_chunks[1]);

                    // CPU Gauge
                    let cpu_label = format!(
                        "{:.1}% (Load: {:.2}, {:.2}, {:.2})",
                        snap.cpu.total_pct,
                        snap.cpu.load_avg[0],
                        snap.cpu.load_avg[1],
                        snap.cpu.load_avg[2]
                    );
                    let cpu_gauge = Gauge::default()
                        .block(Block::default().title("CPU").borders(Borders::ALL))
                        .gauge_style(Style::default().fg(Color::Green))
                        .percent((snap.cpu.total_pct as u16).min(100))
                        .label(cpu_label);
                    f.render_widget(cpu_gauge, top_chunks[0]);

                    // Memory Gauge
                    let used_mb = snap.memory.used_bytes / (1024 * 1024);
                    let total_mb = snap.memory.total_bytes / (1024 * 1024);
                    let mem_pct = if total_mb > 0 {
                        ((used_mb as f64 / total_mb as f64) * 100.0) as u16
                    } else {
                        0
                    };
                    let mem_label = format!("{used_mb} MB / {total_mb} MB");
                    let mem_gauge = Gauge::default()
                        .block(Block::default().title("Memory").borders(Borders::ALL))
                        .gauge_style(Style::default().fg(Color::Yellow))
                        .percent(mem_pct.min(100))
                        .label(mem_label);
                    f.render_widget(mem_gauge, top_chunks[1]);

                    let bottom_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(main_chunks[2]);

                    let io_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(bottom_chunks[0]);

                    // Disk Table
                    // ⚡ Bolt Optimization: Removed intermediate `.collect::<Vec<_>>()` allocation by passing
                    // the iterator directly to `Table::new`. We also replaced `vec![...]` with
                    // static arrays `[...]` for `Row::new` to eliminate heap allocations per frame on the UI hot path.
                    let disk_table = Table::new(
                        snap.disks.iter().map(|d| {
                            Row::new([
                                Cell::from(d.name.as_str()),
                                Cell::from(format!("{}", d.read_bytes / 1024)),
                                Cell::from(format!("{}", d.write_bytes / 1024)),
                            ])
                        }),
                        [
                            Constraint::Percentage(40),
                            Constraint::Percentage(30),
                            Constraint::Percentage(30),
                        ],
                    )
                    .header(
                        Row::new(["Device", "Read KB", "Write KB"])
                            .style(Style::default().add_modifier(Modifier::BOLD)),
                    )
                    .block(Block::default().title("Disks").borders(Borders::ALL));
                    f.render_widget(disk_table, io_chunks[0]);

                    // Network Table
                    // ⚡ Bolt Optimization: Replaced `vec![...]` with arrays for zero-allocation rows.
                    let net_table = Table::new(
                        snap.networks.iter().map(|n| {
                            Row::new([
                                Cell::from(n.interface.as_str()),
                                Cell::from(format!("{}", n.rx_bytes / 1024)),
                                Cell::from(format!("{}", n.tx_bytes / 1024)),
                            ])
                        }),
                        [
                            Constraint::Percentage(40),
                            Constraint::Percentage(30),
                            Constraint::Percentage(30),
                        ],
                    )
                    .header(
                        Row::new(["Interface", "Rx KB", "Tx KB"])
                            .style(Style::default().add_modifier(Modifier::BOLD)),
                    )
                    .block(Block::default().title("Networks").borders(Borders::ALL));
                    f.render_widget(net_table, io_chunks[1]);

                    // Processes Table
                    // ⚡ Bolt Optimization: Replaced `vec![...]` with arrays for zero-allocation rows.
                    let proc_table = Table::new(
                        snap.processes.iter().map(|p| {
                            Row::new([
                                Cell::from(p.pid.to_string()),
                                Cell::from(p.name.as_str()),
                                Cell::from(format!("{:.1}%", p.cpu_pct)),
                                Cell::from(format!("{} MB", p.rss_bytes / (1024 * 1024))),
                            ])
                        }),
                        [
                            Constraint::Percentage(15),
                            Constraint::Percentage(45),
                            Constraint::Percentage(20),
                            Constraint::Percentage(20),
                        ],
                    )
                    .header(
                        Row::new(["PID", "Name", "CPU", "RSS"])
                            .style(Style::default().add_modifier(Modifier::BOLD)),
                    )
                    .block(
                        Block::default()
                            .title("Top Processes")
                            .borders(Borders::ALL),
                    );
                    f.render_widget(proc_table, bottom_chunks[1]);
                } else if last_err.is_none() {
                    let loading = Paragraph::new("Loading metrics...")
                        .block(Block::default().borders(Borders::ALL));
                    f.render_widget(loading, main_chunks[1]);
                }
            })?;

            // Event handling
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            #[allow(clippy::collapsible_if)]
            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    #[allow(clippy::if_same_then_else)]
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        break;
                    } else if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(event::KeyModifiers::CONTROL)
                    {
                        break;
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
                // Fetch metrics
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    args.tcp,
                    Method::GET,
                    &format!("/vms/{}/metrics", args.id),
                    None,
                )
                .await?;

                if status.is_success() {
                    match serde_json::from_str::<hitz_api::MetricsSnapshot>(&resp) {
                        Ok(snap) => {
                            last_snap = Some(snap);
                            last_err = None;
                        }
                        Err(e) => {
                            last_err = Some(format!("Parse error: {e}"));
                        }
                    }
                } else {
                    last_err = Some(format_error_response(status, &resp, "Error"));
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    // Restore terminal
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;

    res
}
async fn handle_vm_metrics(args: &VmIdArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!(
        "{}",
        format!("⏳ Fetching metrics for VM '{}'...", args.id).cyan()
    );
    let _ = std::io::stdout().flush();
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}/metrics", args.id),
        None,
    )
    .await?;
    if status.is_success() {
        print!("\r\x1b[2K");
        let snap: hitz_api::MetricsSnapshot =
            serde_json::from_str(&resp).context("failed to parse metrics response")?;
        print!("{}", format_metrics_snapshot(&snap));
    } else {
        print!("\r\x1b[2K");
        print_error_response(
            status,
            &resp,
            &format!("Failed to get VM {} metrics", args.id),
        );
    }
    Ok(())
}

async fn handle_vm_record(args: &VmRecordArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::time::Instant;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.out)
        .context("failed to open output file for recording")?;

    let interval = std::time::Duration::from_millis(args.interval_ms);
    let mut ticker = tokio::time::interval(interval);
    let start_time = Instant::now();

    println!(
        "{}",
        format!(
            "⏺ Recording metrics for VM '{}' to '{}' (interval: {}ms)...",
            args.id,
            args.out.display(),
            args.interval_ms
        )
        .cyan()
    );

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);
    ctrlc::set_handler(move || {
        stop_clone.store(true, Ordering::SeqCst);
    })
    .context("failed to set Ctrl-C handler")?;

    let mut samples = 0;

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            println!("{}", "⏹ Recording stopped by user.".yellow());
            break;
        }

        if let Some(duration) = args
            .duration_secs
            .filter(|&d| start_time.elapsed().as_secs() >= d)
        {
            println!(
                "{}",
                format!("⏹ Recording reached duration limit ({duration}s).").yellow()
            );
            break;
        }

        let _ = ticker.tick().await;

        let (status, resp) = pipe_client::pipe_request(
            &args.pipe,
            args.tcp,
            Method::GET,
            &format!("/vms/{}/metrics", args.id),
            None,
        )
        .await?;

        if status.is_success() {
            let snap: hitz_api::MetricsSnapshot =
                serde_json::from_str(&resp).context("failed to parse metrics response")?;
            let mut line = serde_json::to_string(&snap).context("failed to serialize metrics")?;
            line.push('\n');
            file.write_all(line.as_bytes())
                .context("failed to write metrics line to file")?;
            samples += 1;
            print!("\r{}", format!("⏺ Recorded {samples} samples...").green());
            let _ = stdout().flush();
        } else {
            let msg = format_error_response(status, &resp, "Error");
            eprintln!("\n{}", msg.red());
            break;
        }
    }

    println!();
    println!(
        "{}",
        format!("✓ Finished recording {samples} samples.").green()
    );

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_vm_timeline(args: &VmTimelineArgs) -> Result<()> {
    use comfy_table::presets::UTF8_FULL_CONDENSED;
    use comfy_table::{Cell, Color, Table};
    use crossterm::style::Stylize;
    use hitz_api::{HealthCheck, HealthStatus};
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(&args.in_file)
        .context(format!("Failed to open {}", args.in_file.display()))?;
    let reader = BufReader::new(file);

    println!(
        "{}",
        format!(
            "Analyzing health timeline from {}...",
            args.in_file.display()
        )
        .cyan()
    );
    println!();

    let mut current_status = None;
    let mut current_reasons: Vec<String> = Vec::new();

    let mut line_count = 0;
    let mut initial_timestamp = None;

    let mut table = Table::new();
    let _ = table.load_preset(UTF8_FULL_CONDENSED);
    let _ = table.set_header([
        Cell::new("Time").add_attribute(comfy_table::Attribute::Bold),
        Cell::new("Status").add_attribute(comfy_table::Attribute::Bold),
        Cell::new("Reasons Added").add_attribute(comfy_table::Attribute::Bold),
        Cell::new("Reasons Removed").add_attribute(comfy_table::Attribute::Bold),
    ]);

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.context("Failed to read line from file")?;
        if line.trim().is_empty() {
            continue;
        }

        let snap: hitz_api::MetricsSnapshot = match serde_json::from_str(&line) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "{}",
                    format!(
                        "✗ Invalid or corrupted metrics data on line {}: {}",
                        line_num + 1,
                        e
                    )
                    .red()
                );
                continue;
            }
        };

        if initial_timestamp.is_none() {
            initial_timestamp = Some(snap.timestamp_ms);
        }

        line_count += 1;

        let health = snap.assess_health();
        let status_changed = current_status != Some(health.status);
        let reasons_changed = current_reasons != health.reasons;

        if status_changed || reasons_changed {
            let elapsed_ms = snap
                .timestamp_ms
                .saturating_sub(initial_timestamp.unwrap_or(snap.timestamp_ms));
            let elapsed_secs = elapsed_ms / 1000;
            let time_str = format!("T+{:02}:{:02}", elapsed_secs / 60, elapsed_secs % 60);

            let status_cell = match health.status {
                HealthStatus::Healthy => Cell::new(" ✅ HEALTHY ")
                    .fg(Color::White)
                    .bg(Color::DarkGreen)
                    .add_attribute(comfy_table::Attribute::Bold),
                HealthStatus::Warning => Cell::new(" ⚠️ WARN ")
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_attribute(comfy_table::Attribute::Bold),
                HealthStatus::Critical => Cell::new(" 🚨 CRIT ")
                    .fg(Color::White)
                    .bg(Color::DarkRed)
                    .add_attribute(comfy_table::Attribute::Bold),
            };

            // ⚡ Bolt Optimization: Replace intermediate `Vec` heap allocations and `.join(...)`
            // with an iterator chain using `.fold(String::new(), ...)` to eliminate
            // intermediate heap allocations and multiple `format!` calls when formatting reasons.
            let added_reasons = health
                .reasons
                .iter()
                .filter(|reason| !current_reasons.contains(*reason))
                .fold(String::new(), |mut acc, reason| {
                    use std::fmt::Write;
                    if !acc.is_empty() {
                        let _ = write!(acc, "\n");
                    }
                    let _ = write!(acc, "+ {reason}");
                    acc
                });

            let removed_reasons = current_reasons
                .iter()
                .filter(|reason| !health.reasons.contains(*reason))
                .fold(String::new(), |mut acc, reason| {
                    use std::fmt::Write;
                    if !acc.is_empty() {
                        let _ = write!(acc, "\n");
                    }
                    let _ = write!(acc, "- {reason}");
                    acc
                });

            if status_changed || !added_reasons.is_empty() || !removed_reasons.is_empty() {
                let _ = table.add_row([
                    Cell::new(time_str),
                    status_cell,
                    Cell::new(added_reasons),
                    Cell::new(removed_reasons),
                ]);
            }

            current_status = Some(health.status);
            current_reasons = health.reasons;
        }
    }

    if table.row_iter().count() > 0 {
        println!("{table}");
    } else {
        println!("{}", "No health transitions detected.".yellow());
    }

    println!();
    println!(
        "{}",
        format!("✓ Analysis complete. Processed {line_count} samples.").green()
    );

    Ok(())
}

async fn handle_vm_export_metrics(args: &VmExportArgs) -> Result<()> {
    use crossterm::style::Stylize;
    use std::io::Write;
    print!(
        "{}",
        format!("⏳ Exporting metrics for VM '{}'...", args.id).cyan()
    );
    let _ = std::io::stdout().flush();
    let (status, resp) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}/metrics", args.id),
        None,
    )
    .await?;
    if status.is_success() {
        use crossterm::style::Stylize;
        let snap: hitz_api::MetricsSnapshot =
            serde_json::from_str(&resp).context("failed to parse metrics response")?;
        let json = serde_json::to_string_pretty(&snap).context("failed to serialize metrics")?;
        std::fs::write(&args.out, json).context("failed to write metrics export to file")?;
        println!(
            "\r\x1b[2K{}",
            format!("✓ Exported metrics to {}", args.out.display()).green()
        );
    } else {
        print_action_result(
            status,
            &resp,
            "",
            &format!("Failed to export VM {} metrics", args.id),
        );
    }
    Ok(())
}

/// Execute a `vm` subcommand by talking to the daemon over the named pipe.
fn run_vm_command(cmd: VmCommand) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    rt.block_on(async {
        match cmd {
            VmCommand::Create(args) => handle_vm_create(&args).await,
            VmCommand::Clone(args) => handle_vm_clone(&args).await,
            VmCommand::Start(args) => handle_vm_start(&args).await,
            VmCommand::Stop(args) => handle_vm_stop(&args).await,
            VmCommand::Restart(args) => handle_vm_restart(&args).await,
            VmCommand::Status(args) => handle_vm_status(&args).await,
            VmCommand::List(args) => handle_vm_list(&args).await,
            VmCommand::Delete(args) => handle_vm_delete(&args).await,
            VmCommand::Serial(args) => handle_vm_serial(&args).await,
            VmCommand::Metrics(args) => handle_vm_metrics(&args).await,
            VmCommand::Top(args) => handle_vm_top(&args).await,
            VmCommand::Dashboard(args) => handle_vm_dashboard(&args).await,
            VmCommand::ExportMetrics(args) => handle_vm_export_metrics(&args).await,
            VmCommand::Record(args) => handle_vm_record(&args).await,
            VmCommand::Analyze(args) => handle_vm_analyze(&args).await,
            VmCommand::Timeline(args) => handle_vm_timeline(&args),
        }
    })
}

#[allow(clippy::too_many_lines)]
async fn handle_vm_analyze(args: &VmIdArgs) -> Result<()> {
    use crossterm::style::Stylize;

    print!("{}", format!("⏳ Analyzing VM '{}'...", args.id).cyan());
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // Fetch VM Info
    let (status_info, resp_info) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}", args.id),
        None,
    )
    .await?;

    if !status_info.is_success() {
        print_error_response(
            status_info,
            &resp_info,
            &format!("Failed to get VM {} info", args.id),
        );
        return Ok(());
    }

    let info: hitz_api::VmInfo = match serde_json::from_str(&resp_info) {
        Ok(i) => i,
        Err(e) => {
            print_error_response(
                status_info,
                &resp_info,
                &format!("Failed to parse VM {} info ({e})", args.id),
            );
            return Ok(());
        }
    };

    if info.state != hitz_api::VmState::Running {
        println!(
            "{}",
            format!(
                "VM is not running (State: {:?}). Metrics analysis requires a running VM.",
                info.state
            )
            .yellow()
        );
        return Ok(());
    }

    // Fetch Metrics
    let (status_metrics, resp_metrics) = pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}/metrics", args.id),
        None,
    )
    .await?;

    if !status_metrics.is_success() {
        print_error_response(
            status_metrics,
            &resp_metrics,
            &format!("Failed to get VM {} metrics", args.id),
        );
        return Ok(());
    }

    let metrics: hitz_api::MetricsSnapshot = match serde_json::from_str(&resp_metrics) {
        Ok(m) => m,
        Err(e) => {
            print_error_response(
                status_metrics,
                &resp_metrics,
                &format!("Failed to parse VM {} metrics ({e})", args.id),
            );
            return Ok(());
        }
    };

    let insights = analyzer::analyze_vm(&info, &metrics);

    println!(
        "\r\x1b[2K{} {}",
        "✅".green(),
        format!("Analyzed VM '{}'", args.id).cyan()
    );

    println!(
        "\n{}",
        format!(" 🔍 Analysis Report for VM '{}' ", args.id)
            .bold()
            .on_blue()
            .white()
    );
    println!();

    if insights.is_empty() {
        println!("{}", "  ✅ System is healthy. No issues detected.".green());
    } else {
        use comfy_table::presets::UTF8_FULL_CONDENSED;
        use comfy_table::{Cell, Color, Table};
        let mut table = Table::new();
        let _ = table.load_preset(UTF8_FULL_CONDENSED);

        let _ = table.set_header([
            Cell::new("Level").add_attribute(comfy_table::Attribute::Bold),
            Cell::new("Insight").add_attribute(comfy_table::Attribute::Bold),
        ]);

        for insight in insights {
            let (level_cell, msg_cell) = match insight.level {
                analyzer::WarningLevel::Critical => (
                    Cell::new(" 🚨 CRIT ")
                        .fg(Color::White)
                        .bg(Color::DarkRed)
                        .add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(insight.message).fg(Color::Red),
                ),
                analyzer::WarningLevel::Warning => (
                    Cell::new(" ⚠️ WARN ")
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(insight.message).fg(Color::Yellow),
                ),
                analyzer::WarningLevel::Info => (
                    Cell::new(" ℹ️ INFO ")
                        .fg(Color::White)
                        .bg(Color::Blue)
                        .add_attribute(comfy_table::Attribute::Bold),
                    Cell::new(insight.message),
                ),
            };
            let _ = table.add_row([level_cell, msg_cell]);
        }
        println!("{table}");
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn handle_vm_dashboard(args: &VmListArgs) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::{
        Terminal,
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    };
    use std::time::{Duration, Instant};

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let tick_rate = Duration::from_secs(1);
    let mut last_tick = Instant::now();
    let mut last_fetch = last_tick
        .checked_sub(Duration::from_secs(10))
        .unwrap_or(last_tick);
    let mut vms: Vec<hitz_api::VmInfo> = Vec::new();
    let mut err_msg: Option<String> = None;

    let mut should_quit = false;

    while !should_quit {
        let now = Instant::now();

        if now.duration_since(last_fetch) >= tick_rate {
            last_fetch = now;
            match pipe_client::pipe_request(&args.pipe, args.tcp, Method::GET, "/vms", None).await {
                Ok((status, resp)) => {
                    if status.is_success() {
                        if let Ok(fetched) = serde_json::from_str::<Vec<hitz_api::VmInfo>>(&resp) {
                            vms = fetched;
                            err_msg = None;
                        } else {
                            err_msg = Some("Failed to parse JSON".to_string());
                        }
                    } else {
                        err_msg = Some(format_error_response(status, &resp, "Error"));
                    }
                }
                Err(e) => {
                    err_msg = Some(format!("Request failed: {e}"));
                }
            }
        }

        let _ = terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(5)].as_ref())
                .split(f.area());

            let mut header_text =
                " Hitz VM Dashboard | Polling GET /vms | Press 'q' or 'ESC' to exit ".to_string();
            if let Some(ref e) = err_msg {
                use std::fmt::Write;
                let _ = write!(header_text, "| Error: {e}");
            }

            let header = Paragraph::new(header_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Dashboard ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            );
            f.render_widget(header, chunks[0]);

            if vms.is_empty() {
                let no_vms_msg = Paragraph::new("ℹ️  No VMs running.")
                    .style(Style::default().fg(Color::Blue))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Virtual Machines "),
                    );
                f.render_widget(no_vms_msg, chunks[1]);
            } else {
                let rows = vms.iter().map(|vm| {
                    let state_str = match vm.state {
                        hitz_api::VmState::Running => "Running",
                        hitz_api::VmState::Stopped => "Stopped",
                        hitz_api::VmState::Failed => "Failed",
                        hitz_api::VmState::Created => "Created",
                    };
                    let state_color = match vm.state {
                        hitz_api::VmState::Running => Color::Green,
                        hitz_api::VmState::Stopped => Color::Yellow,
                        hitz_api::VmState::Failed => Color::Red,
                        hitz_api::VmState::Created => Color::Cyan,
                    };
                    let exit_reason = vm.exit_reason.as_deref().unwrap_or("-");

                    // ⚡ Bolt Optimization:
                    // Replaced `vm.id.clone()` with `vm.id.as_str()` when creating `Cell`s.
                    // This removes an unnecessary String heap allocation per VM in the hot TUI
                    // rendering loop, which runs several times per second. `Cell::from` correctly
                    // accepts `&str`.
                    Row::new([
                        Cell::from(vm.id.as_str()),
                        Cell::from(state_str).style(Style::default().fg(state_color)),
                        Cell::from(vm.config.ram_mib.to_string()),
                        Cell::from(vm.config.cpus.to_string()),
                        Cell::from(exit_reason),
                    ])
                });

                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(20),
                        Constraint::Length(10),
                        Constraint::Length(10),
                        Constraint::Length(6),
                        Constraint::Min(20),
                    ],
                )
                .header(
                    Row::new(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Virtual Machines "),
                );

                f.render_widget(table, chunks[1]);
            }
        });

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout).unwrap_or(false) {
            if let Event::Key(key) = event::read().unwrap_or(Event::FocusGained) {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    should_quit = true;
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    Ok(())
}

#[cfg(test)]
#[allow(
    unsafe_code,
    clippy::items_after_statements,
    clippy::ignore_without_reason
)]
mod tests {
    use super::*;

    /// Mutex that serialises any test touching `OTEL_EXPORTER_OTLP_ENDPOINT`
    /// so parallel test threads cannot race on environment state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_cli_wins_over_env() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let cli_val = "http://cli-endpoint:4317".to_string();
        let env_val = "http://env-endpoint:4317";
        // SAFETY: single-threaded test, no other env manipulation concurrent
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", env_val);
        }
        let resolved = resolve_otlp_endpoint(Some(cli_val.clone()));
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        assert_eq!(resolved, Some(cli_val));
    }

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_falls_back_to_env() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let env_val = "http://env-endpoint:4317".to_string();
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", &env_val);
        }
        let resolved = resolve_otlp_endpoint(None);
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        assert_eq!(resolved, Some(env_val));
    }

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_none_when_absent() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        let resolved = resolve_otlp_endpoint(None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn parse_port_forward_valid() {
        let pf = parse_port_forward("2222:22").expect("valid");
        assert_eq!(pf.host_port, 2222);
        assert_eq!(pf.guest_port, 22);
    }

    #[test]
    fn parse_port_forward_missing_colon() {
        assert!(parse_port_forward("2222").is_err());
    }

    #[test]
    fn parse_port_forward_bad_host() {
        assert!(parse_port_forward("abc:22").is_err());
    }

    #[test]
    fn parse_port_forward_bad_guest() {
        assert!(parse_port_forward("2222:xyz").is_err());
    }

    #[test]
    fn build_launch_args_round_trip() {
        let original = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: r"\\.\pipe\hitz-test".to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch_args = build_launch_args(&original.start).expect("valid UTF-8 path");
        // SCM-only fields must NOT be forwarded to the daemon process.
        assert!(
            !launch_args.iter().any(|a| a == "--auto-start"),
            "--auto-start must not be forwarded; it is SCM-only"
        );
        assert!(
            !launch_args.iter().any(|a| a == "--display-name"),
            "--display-name must not be forwarded; it is SCM-only"
        );
        assert!(
            !launch_args.iter().any(|a| a == "--description"),
            "--description must not be forwarded; it is SCM-only"
        );
        let mut argv = vec![std::ffi::OsString::from("hitz")];
        argv.extend(launch_args.into_iter().map(std::ffi::OsString::from));
        let cli = Cli::try_parse_from(argv).expect("re-parse failed");
        let Command::Daemon(DaemonCommand::Start(recovered)) = cli.command else {
            panic!("expected daemon start");
        };
        assert_eq!(recovered.pipe, original.start.pipe);
        assert_eq!(recovered.state_dir, original.start.state_dir);
        assert_eq!(recovered.verbose, original.start.verbose);
    }

    #[test]
    fn build_launch_args_includes_pipe_and_state_dir() {
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: r"\\.\pipe\custom".to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"D:\vms")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args.start).expect("valid UTF-8 path");
        assert!(launch.contains(&"--pipe".to_string()));
        assert!(launch.contains(&r"\\.\pipe\custom".to_string()));
        assert!(launch.contains(&"--state-dir".to_string()));
        assert!(launch.contains(&r"D:\vms".to_string()));
    }

    #[test]
    fn build_launch_args_verbose_flag() {
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: true,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args.start).expect("valid UTF-8 path");
        assert!(launch.contains(&"--verbose".to_string()));
    }

    /// Requires admin privileges and `HITZ_TEST_SERVICE=1` env var.
    /// Run: `cargo test -p hitz-cli svc_ -- --ignored --test-threads=1`
    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_install_and_remove() {
        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        // Clean up any leftover from a previous run.
        let _ = remove_service();

        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms-test")),
            },
            auto_start: false,
            display_name: Some("Hitz test service".to_string()),
            description: Some("Integration test".to_string()),
        };
        install_service(&args).expect("install failed");

        // Verify entry exists in SCM.
        let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .expect("open SCM");
        let svc = mgr.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS);
        assert!(svc.is_ok(), "service not found after install");

        remove_service().expect("remove failed");

        // Verify entry is gone.
        let svc2 = mgr.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS);
        assert!(svc2.is_err(), "service still exists after remove");
    }

    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_install_idempotent_error() {
        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        let _ = remove_service();
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms-test")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        install_service(&args).expect("first install failed");
        let result = install_service(&args);
        // Cleanup before asserting so we don't leave the service installed on failure.
        remove_service().expect("cleanup remove failed");
        assert!(result.is_err(), "second install should fail");
    }

    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_remove_nonexistent() {
        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        let _ = remove_service(); // ensure not installed
        let result = remove_service();
        assert!(
            result.is_err(),
            "remove of non-existent service should error"
        );
    }

    #[test]
    fn metrics_output_formats_snapshot() {
        use hitz_api::{CpuMetrics, DiskMetrics, MemoryMetrics, MetricsSnapshot, NetMetrics};
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.42, 0.38, 0.31],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 128 * 1024 * 1024,
                free_bytes: 128 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".into(),
                reads_total: 100,
                writes_total: 50,
                read_bytes: 512 * 1024,
                write_bytes: 256 * 1024,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".into(),
                rx_bytes: 1_048_576,
                tx_bytes: 524_288,
                rx_packets: 1000,
                tx_packets: 500,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        };
        let output = format_metrics_snapshot(&snap);
        assert!(output.contains("12.5%"), "missing CPU pct: {output}");
        assert!(
            output.contains("128 MiB / 256 MiB"),
            "missing memory: {output}"
        );
        assert!(output.contains("vda"), "missing disk: {output}");
        assert!(output.contains("eth0"), "missing network: {output}");
        assert!(
            output.contains("System:"),
            "missing system header: {output}"
        );
        assert!(output.contains("Disks:"), "missing disks header: {output}");
        assert!(
            output.contains("Networks:"),
            "missing networks header: {output}"
        );
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

#[cfg(test)]
mod top_tests {
    use crate::{VmExportArgs, VmIdArgs};
    use std::path::PathBuf;

    // Just a sanity check to verify the compiler parses everything.
    // Testing terminal UI without a real terminal attached can block/panic,
    // so we just assert our command layout exists and compiles correctly.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_top_command_definition_compiles() {
        let _args = VmIdArgs {
            id: "test".to_string(),
            pipe: "pipe".to_string(),
            tcp: None,
        };
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_record_command_definition_compiles() {
        let _args = crate::VmRecordArgs {
            id: "test".to_string(),
            out: PathBuf::from("metrics.jsonl"),
            interval_ms: 1000,
            duration_secs: None,
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_export_metrics_command_definition_compiles() {
        let _args = VmExportArgs {
            id: "test".to_string(),
            out: PathBuf::from("metrics.json"),
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_dashboard_command_definition_compiles() {
        let _args = crate::VmListArgs {
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_timeline_command_definition_compiles() {
        let _args = crate::VmTimelineArgs {
            in_file: PathBuf::from("metrics.jsonl"),
        };
        assert!(true);
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_port_forward() {
        let pf = parse_port_forward("8080:80").unwrap();
        assert_eq!(pf.host_port, 8080);
        assert_eq!(pf.guest_port, 80);

        assert!(parse_port_forward("8080").is_err());
        assert!(parse_port_forward("8080:abc").is_err());
        assert!(parse_port_forward("abc:80").is_err());
        assert!(parse_port_forward("70000:80").is_err()); // > 65535
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}
