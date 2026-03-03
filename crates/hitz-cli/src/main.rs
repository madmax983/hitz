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
            DaemonCommand::Start(ref args) => match run_daemon(args) {
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
fn run_daemon(args: &DaemonStartArgs) -> Result<()> {
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
