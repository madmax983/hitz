//! Reusable VM boot pipeline.
//!
//! Extracts the 17-step boot sequence from integration tests into a single
//! `boot_and_run` function that can be called from the CLI, daemon, or tests.

use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hitz_api::VmConfig;
use hitz_boot::{
    BOOT_PARAMS_GPA, CMDLINE_GPA, RSDP_GPA, build_boot_params, build_madt, build_page_tables,
    build_rsdp, build_xsdt, load_elf, load_initramfs, set_acpi_rsdp, set_initramfs_params,
};
use hitz_devices::MmioBus;
use hitz_devices::SerialDevice;
use hitz_devices::VirtioBlockDevice;
use hitz_devices::VirtioMmioTransport;
use hitz_devices::VirtioNetDevice;
use hitz_devices::VirtioVsockDevice;
use hitz_devices::VsockPacket;
use hitz_hal::{
    Gpa, GuestMemAccess, Hypervisor, MemFlags, MemSizeMiB, Partition, PartitionConfig, Vcpu, VcpuId,
};
use hitz_net::{parse_cidr, parse_mac, random_mac};

use crate::boot_regs;
use crate::memory::GuestMemory;
use crate::run_loop::{self, ExitReason, SharedDevices};

/// Virtio-MMIO base address for the first device slot.
const VIRTIO_MMIO_BASE: u64 = 0xD000_0000;
/// Size of each virtio-MMIO slot.
const VIRTIO_MMIO_SIZE: u64 = 0x1000;
/// IRQ vector for the first virtio device (block).
const VIRTIO_IRQ_BASE: u8 = 5;
/// IRQ vector for the virtio-net device.
const VIRTIO_IRQ_NET: u8 = 6;
/// IRQ vector for the virtio-vsock device (MMIO slot 2).
const VIRTIO_IRQ_VSOCK: u8 = 7;

/// Minimum RAM in MiB (kernel + page tables + `boot_params` need at least 2 MiB).
const MIN_RAM_MIB: u32 = 2;

/// Optional channel endpoints pre-created by the daemon before `boot_and_run`.
///
/// Allows the host-side async runtime to communicate with the vsock device
/// during VM execution. When `vsock_channels` is `Some`, the device is wired
/// into MMIO slot 2 at `VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2` (IRQ 7).
///
/// The daemon creates both pairs before `spawn_blocking`, retaining the
/// host-facing ends and passing the device-facing ends here.
pub struct BootExtras {
    /// Device-facing vsock channel ends:
    /// - `.0`: receiver for host→guest RX packets (device reads from this)
    /// - `.1`: sender for guest→host TX packets (device writes to this)
    ///
    /// If `None`, no vsock device is instantiated regardless of `VmConfig`.
    pub vsock_channels: Option<(
        crossbeam_channel::Receiver<VsockPacket>,
        crossbeam_channel::Sender<VsockPacket>,
    )>,
}

impl BootExtras {
    /// No vsock channels — used when the guest agent is disabled or the
    /// caller (CLI, tests) does not need vsock.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            vsock_channels: None,
        }
    }
}

/// Errors that can occur during VM boot or execution.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    /// I/O error (reading kernel, initramfs, disk).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Guest memory error.
    #[error("memory error: {0}")]
    Mem(#[from] crate::error::MemError),

    /// HAL (hypervisor) error.
    #[error("hypervisor error: {0}")]
    Hal(#[from] hitz_hal::HalError),

    /// Boot protocol error.
    #[error("boot error: {0}")]
    Boot(#[from] hitz_boot::BootError),

    /// Configuration validation error.
    #[error("config error: {0}")]
    Config(String),
}

/// Result of running a VM to completion.
#[derive(Debug)]
pub struct VmRunResult {
    /// Why the VM stopped.
    pub exit_reason: ExitReason,
}

/// Validate a `VmConfig` before booting.
pub fn validate_config(config: &VmConfig) -> Result<(), VmError> {
    if !config.kernel_path.exists() {
        return Err(VmError::Config(format!(
            "kernel not found: {}",
            config.kernel_path.display()
        )));
    }
    if let Some(ref path) = config.initramfs_path
        && !path.exists()
    {
        return Err(VmError::Config(format!(
            "initramfs not found: {}",
            path.display()
        )));
    }
    if let Some(ref path) = config.disk_path
        && !path.exists()
    {
        return Err(VmError::Config(format!(
            "disk image not found: {}",
            path.display()
        )));
    }
    if config.ram_mib < MIN_RAM_MIB {
        return Err(VmError::Config(format!(
            "RAM must be at least {MIN_RAM_MIB} MiB, got {}",
            config.ram_mib
        )));
    }
    if config.cpus == 0 || config.cpus > 255 {
        return Err(VmError::Config(format!(
            "cpus must be 1..=255, got {}",
            config.cpus
        )));
    }
    Ok(())
}

/// Cancel all vCPUs using pre-extracted cancel handles.
///
/// Called by the watchdog thread when the stop flag fires, and by each
/// vCPU thread on terminal exit to unblock sibling vCPUs.
fn cancel_all_vcpus<V: Vcpu>(handles: &[V::CancelHandle]) {
    for h in handles {
        let _ = V::cancel_via(h);
    }
}

/// Boot a Linux kernel and run the VM to completion.
///
/// This is the main entry point for the VMM. It:
/// 1. Validates the config and reads kernel/initramfs from disk
/// 2. Sets up guest memory, page tables, `boot_params`, GDT
/// 3. Loads the kernel ELF (and optional initramfs)
/// 4. Creates a hypervisor partition and vCPU
/// 5. Configures registers for 64-bit long mode
/// 6. Sets up serial console and optional virtio-blk device
/// 7. Runs the vCPU loop until the guest exits
///
/// Serial output goes to `serial_out` (e.g. stdout, `Vec<u8>` for tests).
///
/// # Panics
///
/// Panics if a vCPU thread or the watchdog thread cannot be spawned
/// (OS resource exhaustion).
#[allow(
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::expect_used
)]
pub fn boot_and_run<H: Hypervisor, W: Write + Send + 'static>(
    hypervisor: &H,
    config: &VmConfig,
    serial_out: W,
    stop_flag: Arc<AtomicBool>,
    extras: BootExtras,
) -> Result<VmRunResult, VmError> {
    // ── 1. Validate config ──
    validate_config(config)?;

    // ── 2. Read kernel (+ optional initramfs) from disk ──
    let kernel_bytes = fs::read(&config.kernel_path)?;
    let initramfs_bytes = config.initramfs_path.as_ref().map(fs::read).transpose()?;

    let ram_bytes = u64::from(config.ram_mib) * 1024 * 1024;
    let gib_count = config.ram_mib.div_ceil(1024).max(1);

    // ── 3. Allocate guest memory ──
    let mut guest_mem = GuestMemory::new();
    guest_mem.add_region(Gpa::new(0), ram_bytes as usize)?;

    // ── 4. Build page tables ──
    let (pml4_gpa, page_table_writes) = build_page_tables(gib_count)?;
    for write in &page_table_writes {
        guest_mem.write_slice(write.gpa, &write.data)?;
    }

    // ── 5. Load kernel ELF ──
    let load_result = load_elf(&kernel_bytes, &guest_mem)?;

    // ── 6. Optional initramfs ──
    let mut boot_params = build_boot_params(ram_bytes, Gpa::new(CMDLINE_GPA))?;

    if let Some(ref initramfs_data) = initramfs_bytes {
        let initramfs_result = load_initramfs(
            initramfs_data,
            load_result.kernel_end,
            ram_bytes,
            &guest_mem,
        )?;
        set_initramfs_params(
            &mut boot_params,
            initramfs_result.gpa,
            initramfs_result.size,
        )?;
    }

    // ── 7. Write boot_params to guest memory ──
    guest_mem.write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)?;

    // ── 7b. Write ACPI tables for SMP ──
    if config.cpus > 1 {
        let rsdp = build_rsdp();
        guest_mem.write_slice(Gpa::new(RSDP_GPA), &rsdp)?;

        let xsdt = build_xsdt();
        guest_mem.write_slice(Gpa::new(hitz_boot::XSDT_GPA), &xsdt)?;

        let madt = build_madt(config.cpus)?;
        guest_mem.write_slice(Gpa::new(hitz_boot::MADT_GPA), &madt)?;

        set_acpi_rsdp(&mut boot_params, RSDP_GPA);

        // Re-write boot_params with the RSDP pointer set.
        guest_mem.write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)?;
    }

    // ── 8. Write command line ──
    let mut cmdline = config.effective_cmdline().to_string();

    // Append virtio-net MMIO device descriptor to the kernel command line.
    if config.net.is_some() {
        let net_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
        let _ = write!(
            cmdline,
            " virtio_mmio.device=0x{VIRTIO_MMIO_SIZE:x}@0x{net_base:x}:{VIRTIO_IRQ_NET}"
        );
    }

    // Append static IP configuration for the guest's network interface.
    if let Some(ref net_cfg) = config.net {
        let (guest_ip, _) = parse_cidr(&net_cfg.guest_ip).map_err(VmError::Config)?;
        let (gateway_ip, _) = parse_cidr(&net_cfg.host_ip).map_err(VmError::Config)?;
        let _ = write!(
            cmdline,
            " ip={}.{}.{}.{}::{}.{}.{}.{}:255.255.255.0::eth0:off",
            guest_ip[0],
            guest_ip[1],
            guest_ip[2],
            guest_ip[3],
            gateway_ip[0],
            gateway_ip[1],
            gateway_ip[2],
            gateway_ip[3],
        );
    }

    // Append virtio-vsock MMIO device descriptor when vsock channels are provided.
    if extras.vsock_channels.is_some() {
        let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        let _ = write!(
            cmdline,
            " virtio_mmio.device=0x{VIRTIO_MMIO_SIZE:x}@0x{vsock_base:x}:{VIRTIO_IRQ_VSOCK}"
        );
    }

    let mut cmdline_bytes = cmdline.as_bytes().to_vec();
    cmdline_bytes.push(0); // null-terminate
    guest_mem.write_slice(Gpa::new(CMDLINE_GPA), &cmdline_bytes)?;

    // ── 9. Write GDT ──
    boot_regs::write_gdt(&guest_mem)?;

    // ── 10. Create partition + map memory ──
    let partition_cfg = PartitionConfig {
        vcpu_count: config.cpus,
        memory_size: MemSizeMiB::new(u64::from(config.ram_mib)),
    };
    let mut partition = hypervisor.create_partition(&partition_cfg)?;
    guest_mem.map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)?;

    // ── 11. Create vCPUs ──
    let mut vcpus = Vec::with_capacity(config.cpus as usize);
    for i in 0..config.cpus {
        let mut vcpu = partition.create_vcpu(VcpuId::new(i))?;
        if i == 0 {
            // BSP: configure for kernel entry.
            boot_regs::configure_sregs(&mut vcpu, pml4_gpa)?;
            boot_regs::configure_regs(
                &mut vcpu,
                load_result.entry_point,
                Gpa::new(BOOT_PARAMS_GPA),
            )?;
        }
        // APs (i > 0): WHP xAPIC emulation starts them in wait-for-SIPI state.
        vcpus.push(vcpu);
    }

    // ── 12. Set up serial console ──
    let serial = SerialDevice::new(serial_out);

    // ── 13. Set up MMIO bus + optional virtio-blk ──
    let mut mmio_bus = MmioBus::new();
    let guest_mem_arc: Arc<GuestMemory> = Arc::new(guest_mem);

    if let Some(ref disk_path) = config.disk_path {
        let disk_file = fs::File::open(disk_path)?;
        let block_dev = VirtioBlockDevice::new(disk_file)?;
        let mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let transport = VirtioMmioTransport::new(block_dev, mem, VIRTIO_IRQ_BASE);
        mmio_bus.register(VIRTIO_MMIO_BASE, VIRTIO_MMIO_SIZE, Box::new(transport));
    }

    // ── 13b. Optional virtio-net ──
    //
    // The `net_io_handle` must stay alive until after the run loop exits;
    // its `Drop` impl signals the I/O thread to stop and joins it.
    let net_io_handle: Option<hitz_net::NetIoHandle> = if let Some(ref net_cfg) = config.net {
        let guest_mac = if let Some(ref mac_str) = net_cfg.mac {
            parse_mac(mac_str).map_err(VmError::Config)?
        } else {
            random_mac()
        };

        let gateway_mac: [u8; 6] = [0xAA, 0xBB, 0xCC, 0x00, 0x00, 0x01];
        let (gateway_ip, _) = parse_cidr(&net_cfg.host_ip).map_err(VmError::Config)?;

        let (net_dev, tx_receiver, rx_sender) = VirtioNetDevice::new(guest_mac);

        let mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let net_transport = VirtioMmioTransport::new(net_dev, mem, VIRTIO_IRQ_NET);
        let net_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
        mmio_bus.register(net_base, VIRTIO_MMIO_SIZE, Box::new(net_transport));

        let adapter_name = net_cfg
            .adapter_name
            .clone()
            .unwrap_or_else(|| "hitz-net".to_string());

        let handle = hitz_net::start_net_io(
            &adapter_name,
            &net_cfg.host_ip,
            guest_mac,
            gateway_mac,
            gateway_ip,
            tx_receiver,
            rx_sender,
        )
        .map_err(|e| VmError::Config(format!("network setup: {e}")))?;

        Some(handle)
    } else {
        None
    };

    // ── 13c. Optional virtio-vsock (guest metrics agent) ──────────────────────
    //
    // When the daemon provides pre-created channel ends via `BootExtras`, we
    // construct the device from those channels so the host-side async task can
    // communicate with the guest without crossing the `spawn_blocking` boundary.
    // The device is registered at MMIO slot 2 (0xD000_2000, IRQ 7).
    //
    // If `vsock_channels` is `None` (CLI, tests, or agent disabled), no vsock
    // device is created and the cmdline entry was also skipped above.
    if let Some((rx_receiver, tx_sender)) = extras.vsock_channels {
        let vsock_dev = VirtioVsockDevice::with_channels(config.guest_cid, rx_receiver, tx_sender);
        let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        let vsock_mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let vsock_transport = VirtioMmioTransport::new(vsock_dev, vsock_mem, VIRTIO_IRQ_VSOCK);
        mmio_bus.register(vsock_base, VIRTIO_MMIO_SIZE, Box::new(vsock_transport));
    }

    // ── 14. Run vCPU threads ──
    let devices = Arc::new(Mutex::new(SharedDevices { serial, mmio_bus }));

    if vcpus.len() == 1 {
        // Watchdog cancels the vCPU if stop_flag fires while guest is halted.
        let cancel_handle = vcpus[0].cancel_handle();
        let stop_clone = Arc::clone(&stop_flag);
        let watchdog = std::thread::Builder::new()
            .name("cancel-watchdog".into())
            .spawn(move || {
                while !stop_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                let _ = <<H::Partition as Partition>::Vcpu as Vcpu>::cancel_via(&cancel_handle);
            })
            .expect("spawn watchdog thread");

        let exit_reason =
            run_loop::run_vcpu_loop(&mut vcpus[0], &devices, &*guest_mem_arc, &stop_flag)?;

        // Ensure watchdog exits (set flag so it doesn't spin forever on normal exit).
        stop_flag.store(true, Ordering::Relaxed);
        let _ = watchdog.join();
        drop(net_io_handle);
        return Ok(VmRunResult { exit_reason });
    }

    // Multi-vCPU: spawn a thread per vCPU.
    let cancel_handles: Vec<_> = vcpus.iter().map(Vcpu::cancel_handle).collect();
    let shared_handles = Arc::new(cancel_handles);
    let (exit_tx, exit_rx) = mpsc::channel::<Result<ExitReason, hitz_hal::HalError>>();
    let num_vcpus = vcpus.len();

    // Watchdog: ensures cancel fires even if ALL vCPUs are blocked in run().
    let watchdog = {
        let stop_clone = Arc::clone(&stop_flag);
        let handles_clone = Arc::clone(&shared_handles);
        std::thread::Builder::new()
            .name("cancel-watchdog".into())
            .spawn(move || {
                while !stop_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&handles_clone);
            })
            .expect("spawn watchdog thread")
    };

    let handles: Vec<_> = vcpus
        .into_iter()
        .enumerate()
        .map(|(idx, mut vcpu)| {
            let devs = Arc::clone(&devices);
            let mem = Arc::clone(&guest_mem_arc);
            let stop = Arc::clone(&stop_flag);
            let cancel_handles = Arc::clone(&shared_handles);
            let tx = exit_tx.clone();

            std::thread::Builder::new()
                .name(format!("vcpu-{idx}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_loop::run_vcpu_loop(&mut vcpu, &devs, &*mem, &stop)
                    }));

                    // On terminal exit or panic, stop and cancel all sibling vCPUs.
                    match &result {
                        Ok(
                            Ok(ExitReason::Halt | ExitReason::Shutdown | ExitReason::Unexpected(_))
                            | Err(_),
                        )
                        | Err(_) => {
                            stop.store(true, Ordering::Relaxed);
                            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&cancel_handles);
                        }
                        Ok(Ok(ExitReason::Canceled)) => {
                            // Another vCPU already triggered stop; nothing to do.
                        }
                    }

                    // Convert panic payload into an ExitReason.
                    let exit = match result {
                        Ok(r) => r,
                        Err(payload) => {
                            let msg = payload.downcast_ref::<&str>().map_or_else(
                                || {
                                    payload
                                        .downcast_ref::<String>()
                                        .map_or_else(|| "unknown panic".to_string(), Clone::clone)
                                },
                                |s| (*s).to_string(),
                            );
                            Ok(ExitReason::Unexpected(format!(
                                "vCPU {idx} panicked: {msg}"
                            )))
                        }
                    };

                    let _ = tx.send(exit);
                })
                .expect("spawn vcpu thread")
        })
        .collect();

    // Drop sender so the receiver knows when all threads are done.
    drop(exit_tx);

    // Collect results with a 3-second timeout per vCPU.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut final_reason = ExitReason::Canceled;

    for _ in 0..num_vcpus {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match exit_rx.recv_timeout(remaining) {
            Ok(Ok(reason)) => {
                if final_reason == ExitReason::Canceled {
                    final_reason = reason;
                }
            }
            Ok(Err(e)) => {
                if final_reason == ExitReason::Canceled {
                    final_reason = ExitReason::Unexpected(format!("vCPU error: {e}"));
                }
            }
            Err(_) => {
                tracing::warn!("vCPU thread timed out during shutdown");
                break;
            }
        }
    }

    // Signal watchdog to stop (in case it's still polling).
    stop_flag.store(true, Ordering::Relaxed);
    let _ = watchdog.join();

    // Join vCPU threads — should be near-instant since they sent results already.
    for handle in handles {
        let _ = handle.join();
    }

    drop(net_io_handle);
    Ok(VmRunResult {
        exit_reason: final_reason,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn valid_config(kernel_path: std::path::PathBuf) -> VmConfig {
        VmConfig {
            kernel_path,
            initramfs_path: None,
            disk_path: None,
            ram_mib: 128,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: hitz_api::DEFAULT_GUEST_CID,
            guest_agent: hitz_api::GuestAgentMode::Auto,
        }
    }

    #[test]
    fn validate_config_missing_kernel() {
        let cfg = valid_config("nonexistent_kernel".into());
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("kernel not found"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_missing_initramfs() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.initramfs_path = Some("nonexistent_initramfs.cpio".into());
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("initramfs not found"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_missing_disk() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.disk_path = Some("nonexistent_disk.img".into());
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("disk image not found"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_ram_too_small() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.ram_mib = 1;
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("at least 2 MiB"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_cpus_zero() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.cpus = 0;
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("cpus must be"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_cpus_too_many() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.cpus = 256;
        let err = validate_config(&cfg).expect_err("should return error");
        assert!(
            err.to_string().contains("cpus must be"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_ok() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let cfg = valid_config(tmp.path().to_path_buf());
        validate_config(&cfg).expect("valid config should pass");
    }

    #[test]
    fn vsock_mmio_slot_does_not_overlap_net() {
        // Slot 0 = blk (0xD000_0000), slot 1 = net (0xD000_1000),
        // slot 2 = vsock (0xD000_2000). Verify no overlap.
        let blk = VIRTIO_MMIO_BASE;
        let net = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
        let vsock = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        assert_ne!(blk, net);
        assert_ne!(net, vsock);
        assert_eq!(VIRTIO_IRQ_VSOCK, 7);
    }
}
