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
    ActionVmRequest, CreateVmRequest, DEFAULT_CMDLINE, DEFAULT_CPUS, DEFAULT_GUEST_CID,
    DEFAULT_RAM_MIB, GuestAgentMode, VmAction, VmConfig,
};
use hitz_daemon::TelemetryGuard;
use hitz_vmm::ExitReason;
use hitz_whp::WhpHypervisor;
use hyper::Method;

/// Default named pipe path for the daemon.
const DEFAULT_PIPE: &str = r"\\.\pipe\hitz";

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
/// depend on runtime defaults or `%APPDATA%` being set.
///
/// Fields **not** forwarded — they are SCM-registration-only and have no
/// corresponding `daemon start` flag:
/// - `auto_start` — controls `ServiceStartType`, not daemon behaviour
/// - `display_name` — stored in the service registry, not passed to the process
/// - `description` — same as above
#[allow(dead_code)] // used by install_service once implemented in Task 2
fn build_launch_args(args: &DaemonInstallArgs) -> Result<Vec<String>> {
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

fn install_service(_args: &DaemonInstallArgs) -> Result<()> {
    anyhow::bail!("not yet implemented")
}

fn remove_service() -> Result<()> {
    anyhow::bail!("not yet implemented")
}

/// Arguments for `daemon start`.
#[derive(Parser)]
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
    /// Stream serial console output.
    Serial(VmIdArgs),
    /// Display live resource metrics for a running VM.
    Metrics(VmIdArgs),
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
}

// ── hitz daemon start ──

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
    use tracing_subscriber::prelude::*;

    let endpoint = resolve_otlp_endpoint(args.otlp_endpoint.clone());

    // Init OTel providers BEFORE subscriber registration so the tracer exists
    // when the layer is built.
    let telemetry = TelemetryGuard::init(endpoint);

    {
        use tracing_subscriber::Layer as _;

        let mut layers: Vec<
            Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
        > = Vec::new();

        if args.verbose {
            let fmt = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::filter::EnvFilter::new("hitz=debug"))
                .boxed();
            layers.push(fmt);
        }

        if let Some(otel) = telemetry.tracing_layer() {
            layers.push(otel.boxed());
        }

        tracing_subscriber::registry().with(layers).init();
    }

    eprintln!("hitz: daemon listening on {}", args.pipe);
    if let Some(addr) = args.tcp_listen {
        eprintln!("hitz: TCP listener on {addr}");
    }

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    let result = rt.block_on(async {
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

        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl+C")?;
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
        Ok::<(), anyhow::Error>(())
    });

    // Shut down the runtime BEFORE dropping the OTel guard so the batch
    // exporter can flush pending spans while the async executor is still live.
    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    drop(telemetry);

    result
}

// ── Metrics formatting ──

/// Format a [`hitz_api::MetricsSnapshot`] into a human-readable string for CLI display.
fn format_metrics_snapshot(snap: &hitz_api::MetricsSnapshot) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let cores: Vec<String> = snap
        .cpu
        .per_core
        .iter()
        .map(|p| format!("{p:.1}%"))
        .collect();
    let _ = writeln!(
        out,
        "CPU:     total={:.1}%  cores=[{}]  load={:.2}/{:.2}/{:.2}",
        snap.cpu.total_pct,
        cores.join(", "),
        snap.cpu.load_avg[0],
        snap.cpu.load_avg[1],
        snap.cpu.load_avg[2],
    );

    let used_mib = snap.memory.used_bytes / (1024 * 1024);
    let total_mib = snap.memory.total_bytes / (1024 * 1024);
    let _ = writeln!(out, "Memory:  {used_mib} MiB / {total_mib} MiB");

    for disk in &snap.disks {
        let read_kb = disk.read_bytes / 1024;
        let write_kb = disk.write_bytes / 1024;
        let _ = writeln!(
            out,
            "Disk:    {}  reads={}  writes={}  read={}K  write={}K",
            disk.name, disk.reads_total, disk.writes_total, read_kb, write_kb,
        );
    }

    for net in &snap.networks {
        let rx_kb = net.rx_bytes / 1024;
        let tx_kb = net.tx_bytes / 1024;
        let _ = writeln!(
            out,
            "Net:     {}  rx={}K  tx={}K  rx_pkt={}  tx_pkt={}",
            net.interface, rx_kb, tx_kb, net.rx_packets, net.tx_packets,
        );
    }

    if !snap.processes.is_empty() {
        let _ = writeln!(
            out,
            "Procs:   {:>6}  {:<20} {:>6}  {:>8}",
            "PID", "NAME", "CPU%", "RSS"
        );
        for proc in &snap.processes {
            let rss_mb = proc.rss_bytes / (1024 * 1024);
            let _ = writeln!(
                out,
                "         {:>6}  {:<20} {:>5.1}%  {:>7}M",
                proc.pid, proc.name, proc.cpu_pct, rss_mb,
            );
        }
    }

    out
}

// ── hitz vm * ──

/// Execute a `vm` subcommand by talking to the daemon over the named pipe.
#[allow(clippy::too_many_lines)]
fn run_vm_command(cmd: VmCommand) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    rt.block_on(async {
        match cmd {
            VmCommand::Create(args) => {
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
                let body = serde_json::to_string(&CreateVmRequest { config })
                    .context("serialize request")?;
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    args.tcp,
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
                    args.tcp,
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
                    args.tcp,
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
                    args.tcp,
                    Method::GET,
                    &format!("/vms/{}", args.id),
                    None,
                )
                .await?;
                if status.is_success() {
                    if let Ok(info) = serde_json::from_str::<hitz_api::VmInfo>(&resp) {
                        println!("ID:    {}", info.id);
                        println!("State: {:?}", info.state);
                        if !info.config.ports.is_empty() {
                            let ports: Vec<String> = info
                                .config
                                .ports
                                .iter()
                                .map(|p| format!("0.0.0.0:{} -> {}", p.host_port, p.guest_port))
                                .collect();
                            println!("Ports: {}", ports.join(", "));
                        }
                        if let Some(reason) = &info.exit_reason {
                            println!("Exit:  {reason}");
                        }
                    } else {
                        println!("{status}: {resp}");
                    }
                } else {
                    println!("{status}: {resp}");
                }
            }
            VmCommand::List(args) => {
                let (status, resp) =
                    pipe_client::pipe_request(&args.pipe, args.tcp, Method::GET, "/vms", None)
                        .await?;
                println!("{status}: {resp}");
            }
            VmCommand::Delete(args) => {
                let (status, resp) = pipe_client::pipe_request(
                    &args.pipe,
                    args.tcp,
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
            VmCommand::Serial(args) => {
                use http_body_util::BodyExt as _;

                let resp = pipe_client::request_stream(
                    &args.pipe,
                    args.tcp,
                    &format!("/vms/{}/serial", args.id),
                )
                .await?;

                let status = resp.status();
                let mut body = resp.into_body();

                if !status.is_success() {
                    let collected = body.collect().await.context("read error body")?.to_bytes();
                    let text = String::from_utf8_lossy(&collected);
                    anyhow::bail!("serial stream failed ({status}): {text}");
                }

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
            }
            VmCommand::Metrics(args) => {
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
                    print!("{}", format_metrics_snapshot(&snap));
                } else {
                    println!("{status}: {resp}");
                }
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mutex that serialises any test touching `OTEL_EXPORTER_OTLP_ENDPOINT`
    /// so parallel test threads cannot race on environment state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
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
            pipe: r"\\.\pipe\hitz-test".to_string(),
            tcp_listen: None,
            verbose: false,
            otlp_endpoint: None,
            state_dir: Some(PathBuf::from(r"C:\hitz\vms")),
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch_args = build_launch_args(&original).expect("valid UTF-8 path");
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
        assert_eq!(recovered.pipe, original.pipe);
        assert_eq!(recovered.state_dir, original.state_dir);
        assert_eq!(recovered.verbose, original.verbose);
    }

    #[test]
    fn build_launch_args_includes_pipe_and_state_dir() {
        let args = DaemonInstallArgs {
            pipe: r"\\.\pipe\custom".to_string(),
            tcp_listen: None,
            verbose: false,
            otlp_endpoint: None,
            state_dir: Some(PathBuf::from(r"D:\vms")),
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args).expect("valid UTF-8 path");
        assert!(launch.contains(&"--pipe".to_string()));
        assert!(launch.contains(&r"\\.\pipe\custom".to_string()));
        assert!(launch.contains(&"--state-dir".to_string()));
        assert!(launch.contains(&r"D:\vms".to_string()));
    }

    #[test]
    fn build_launch_args_verbose_flag() {
        let args = DaemonInstallArgs {
            pipe: DEFAULT_PIPE.to_string(),
            tcp_listen: None,
            verbose: true,
            otlp_endpoint: None,
            state_dir: Some(PathBuf::from(r"C:\hitz")),
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args).expect("valid UTF-8 path");
        assert!(launch.contains(&"--verbose".to_string()));
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
    }
}
