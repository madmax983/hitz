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
/// # Abstract
/// Provides external dependencies (like vsock channels) to the VM execution
/// environment.
///
/// # The Hero's Journey
/// ```rust
/// # use hitz_vmm::BootExtras;
/// #
/// // Most consumers can just use the empty default:
/// let extras = BootExtras::none();
/// assert!(extras.vsock_channels.is_none());
/// ```
///
/// # The Fine Print
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
///
/// # Abstract
/// Represents the various failure modes that can occur when configuring,
/// booting, or running a micro-VM.
///
/// # The Hero's Journey
/// ```rust
/// # use hitz_vmm::VmError;
/// # use std::io;
/// #
/// fn handle_error(err: VmError) {
///     match err {
///         VmError::Io(e) => println!("IO failed: {}", e),
///         VmError::Config(msg) => println!("Bad config: {}", msg),
///         _ => println!("Other error: {}", err),
///     }
/// }
///
/// let io_err = VmError::Io(io::Error::new(io::ErrorKind::NotFound, "file missing"));
/// handle_error(io_err);
/// ```
///
/// # The Fine Print
/// Most errors in this enum are wrappers around underlying sub-system errors
/// (like [`std::io::Error`] or `hitz_hal::HalError`). They provide a unified
/// error type for the [`boot_and_run`] pipeline.
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
///
/// # Abstract
/// Encapsulates the outcome of the VM's execution lifecycle. When [`boot_and_run`]
/// completes (either normally or via an error/cancellation), it yields this struct.
///
/// # The Hero's Journey
/// ```rust
/// # use hitz_vmm::VmRunResult;
/// # use hitz_vmm::run_loop::ExitReason;
/// #
/// let result = VmRunResult {
///     exit_reason: ExitReason::Halt,
/// };
///
/// if matches!(result.exit_reason, ExitReason::Halt) {
///     println!("The VM halted gracefully.");
/// }
/// ```
///
/// # The Fine Print
/// The `exit_reason` provides exactly *why* the VM loop terminated.
#[derive(Debug)]
pub struct VmRunResult {
    /// Why the VM stopped.
    pub exit_reason: ExitReason,
}

/// # Abstract
/// Validates a [`VmConfig`] prior to initiating the boot sequence.
///
/// Ensures that all requisite file paths (such as the kernel, initramfs, and disk images)
/// exist on the host filesystem. This prevents the VM from attempting a boot sequence
/// that is doomed to fail due to missing dependencies.
///
/// # The Hero's Journey
/// ```rust
/// # use hitz_api::VmConfig;
/// # use hitz_vmm::validate_config;
/// # use std::path::PathBuf;
/// # use std::fs::File;
/// # use tempfile::TempDir;
/// #
/// # let temp_dir = TempDir::new().unwrap();
/// # let kernel_path = temp_dir.path().join("vmlinux");
/// # File::create(&kernel_path).unwrap();
/// #
/// let mut config = VmConfig {
///     kernel_path,
///     initramfs_path: None,
///     disk_path: None,
///     ram_mib: 256,
///     cpus: 1,
///     cmdline: None,
///     net: None,
///     ports: vec![],
///     guest_cid: 3,
///     guest_agent: hitz_api::GuestAgentMode::Disabled,
/// };
///
/// // Verify the configuration is sound before booting!
/// assert!(validate_config(&config).is_ok());
/// ```
///
/// # The Fine Print
/// Validation is strictly a pre-flight check. It verifies the *existence* of the files,
/// but does not parse them to verify they are valid ELF/bzImage kernels, valid cpio
/// archives, or valid raw disk images.
pub fn validate_config(config: &VmConfig) -> Result<(), VmError> {
    #[allow(dead_code)]
    struct SafePath<'a>(&'a std::path::Path);

    impl<'a> SafePath<'a> {
        fn new(path: &'a std::path::Path) -> Result<Self, VmError> {
            if path.is_absolute() {
                return Err(VmError::Config(format!(
                    "absolute paths are not allowed for security reasons: {}",
                    path.display()
                )));
            }
            if path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(VmError::Config(format!(
                    "path traversal detected: {}",
                    path.display()
                )));
            }
            Ok(Self(path))
        }
    }

    let _kernel_path = SafePath::new(&config.kernel_path)?;
    if let Some(ref path) = config.initramfs_path {
        let _initramfs_path = SafePath::new(path)?;
    }
    if let Some(ref path) = config.disk_path {
        let _disk_path = SafePath::new(path)?;
    }
    if let hitz_api::GuestAgentMode::Custom(ref path) = config.guest_agent {
        let _agent_path = SafePath::new(path)?;
        if !path.exists() {
            return Err(VmError::Config(format!(
                "guest agent not found: {}",
                path.display()
            )));
        }
    }
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
/// # Abstract
/// The central orchestrator for booting and executing a [`VmConfig`].
///
/// This function acts as the main entry point for the VM lifecycle. It handles everything
/// from parsing the kernel and laying out guest memory, to spawning the vCPU thread and
/// routing MMIO/PIO requests.
///
/// # The Hero's Journey
/// ```rust
/// # use hitz_api::VmConfig;
/// # use hitz_vmm::{boot_and_run, BootExtras};
/// # use hitz_hal::{Hypervisor, Partition, PartitionConfig, Vcpu, VcpuExit, HalError, Gpa, VcpuId};
/// # use std::sync::{Arc, atomic::AtomicBool};
/// # use std::io::sink;
/// # use std::path::PathBuf;
/// # use tempfile::TempDir;
/// # use std::fs::File;
/// #
/// # // Define dummy hypervisor structs to satisfy trait bounds
/// # struct DummyHypervisor;
/// # struct DummyPartition;
/// # struct DummyVcpu;
/// # #[derive(Clone)]
/// # struct DummyCancelHandle;
/// # impl Hypervisor for DummyHypervisor {
/// #     type Partition = DummyPartition;
/// #     fn create_partition(&self, _cfg: &PartitionConfig) -> Result<Self::Partition, HalError> { Ok(DummyPartition) }
/// # }
/// # impl Partition for DummyPartition {
/// #     type Vcpu = DummyVcpu;
/// #     unsafe fn map_memory(&mut self, _gpa: Gpa, _hva: *mut u8, _size: usize, _flags: hitz_hal::MemFlags) -> Result<(), HalError> { Ok(()) }
/// #     fn unmap_memory(&mut self, _gpa: Gpa, _size: usize) -> Result<(), HalError> { Ok(()) }
/// #     fn create_vcpu(&mut self, _id: VcpuId) -> Result<Self::Vcpu, HalError> { Ok(DummyVcpu) }
/// #     fn request_interrupt(&self, _id: VcpuId, _vector: u8) -> Result<(), HalError> { Ok(()) }
/// # }
/// # impl Vcpu for DummyVcpu {
/// #     type CancelHandle = DummyCancelHandle;
/// #     fn run(&mut self) -> Result<VcpuExit, HalError> { Ok(VcpuExit::Halt) }
/// #     fn set_regs(&mut self, _regs: &hitz_hal::StandardRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn set_sregs(&mut self, _sregs: &hitz_hal::SpecialRegs) -> Result<(), HalError> { Ok(()) }
/// #     fn get_regs(&self) -> Result<hitz_hal::StandardRegs, HalError> { Ok(Default::default()) }
/// #     fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, HalError> { Ok(Default::default()) }
/// #     fn cancel_handle(&self) -> Self::CancelHandle { DummyCancelHandle }
/// #     fn cancel_via(_h: &Self::CancelHandle) -> Result<(), HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _vector: u8) -> Result<(), HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), HalError> { Ok(()) }
/// # }
/// #
/// # let temp_dir = TempDir::new().unwrap();
/// # let kernel_path = temp_dir.path().join("vmlinux");
/// # File::create(&kernel_path).unwrap(); // create dummy kernel
/// #
/// let mut config = VmConfig {
///     kernel_path,
///     initramfs_path: None,
///     disk_path: None,
///     ram_mib: 256,
///     cpus: 1,
///     cmdline: None,
///     net: None,
///     ports: vec![],
///     guest_cid: 3,
///     guest_agent: hitz_api::GuestAgentMode::Disabled,
/// };
///
/// let hypervisor = DummyHypervisor;
/// let stop_flag = Arc::new(AtomicBool::new(false));
/// let extras = BootExtras::none();
///
/// // Start the machine! (using `sink()` to discard serial output)
/// // let result = boot_and_run(&hypervisor, &config, sink(), stop_flag, extras);
/// ```
///
/// # The Fine Print
/// Serial output goes to `serial_out` (e.g., `stdout` or a `Vec<u8>` for tests).
///
/// ## Panics
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
    mut extras: BootExtras,
) -> Result<VmRunResult, VmError> {
    // ── 1. Validate config ──
    validate_config(config)?;

    let ram_bytes = u64::from(config.ram_mib) * 1024 * 1024;
    let gib_count = config.ram_mib.div_ceil(1024).max(1);

    // ── 2-7. Load Guest Memory ──
    let mut guest_mem = GuestMemory::with_capacity(4); // Pre-allocate typical region count
    let (pml4_gpa, load_result) = setup_guest_memory(&mut guest_mem, config, ram_bytes, gib_count)?;

    // ── 8. Write command line ──
    build_kernel_cmdline(&mut guest_mem, config, &extras)?;

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
    let entry_point = load_result.entry_point;
    let mut vcpus = Vec::with_capacity(config.cpus as usize);
    for i in 0..config.cpus {
        let mut vcpu = partition.create_vcpu(VcpuId::new(i))?;
        if i == 0 {
            boot_regs::configure_sregs(&mut vcpu, pml4_gpa)?;
            boot_regs::configure_regs(&mut vcpu, entry_point, Gpa::new(BOOT_PARAMS_GPA))?;
        }
        vcpus.push(vcpu);
    }

    // ── 12-13. Set up devices ──
    let serial = SerialDevice::new(serial_out);
    let mut mmio_bus = MmioBus::new();
    let guest_mem_arc: Arc<GuestMemory> = Arc::new(guest_mem);

    let net_io_handle = setup_devices(&mut mmio_bus, config, &mut extras, &guest_mem_arc)?;
    let devices = Arc::new(Mutex::new(SharedDevices { serial, mmio_bus }));

    // ── 14. Run vCPU threads ──
    let exit_reason = if vcpus.len() == 1 {
        // ⚡ Bolt Optimization: Eliminated `.to_string()` allocation on error path.
        let single_vcpu = vcpus.pop().ok_or_else(|| {
            std::io::Error::other(
                "vcpus array is unexpectedly empty when it should have 1 element",
            )
        })?;
        run_single_vcpu::<H>(single_vcpu, &devices, &guest_mem_arc, &stop_flag)?
    } else {
        run_multi_vcpu::<H>(vcpus, &devices, &guest_mem_arc, &stop_flag)
    };

    drop(net_io_handle);
    Ok(VmRunResult { exit_reason })
}

// --- Helper Functions ---

#[allow(clippy::needless_pass_by_ref_mut)]
fn setup_guest_memory(
    guest_mem: &mut GuestMemory,
    config: &VmConfig,
    ram_bytes: u64,
    gib_count: u32,
) -> Result<(Gpa, hitz_boot::KernelLoadResult), VmError> {
    guest_mem.add_region(
        Gpa::new(0),
        usize::try_from(ram_bytes).unwrap_or(usize::MAX),
    )?;

    let (pml4_gpa, page_table_writes) = build_page_tables(gib_count)?;
    for write in &page_table_writes {
        guest_mem.write_slice(write.gpa, &write.data)?;
    }

    let kernel_bytes = fs::read(&config.kernel_path)?;
    let load_result = load_elf(&kernel_bytes, guest_mem)?;

    let mut boot_params = build_boot_params(ram_bytes, Gpa::new(CMDLINE_GPA))?;

    if let Some(ref initramfs_path) = config.initramfs_path {
        let initramfs_data = fs::read(initramfs_path)?;
        let initramfs_result = load_initramfs(
            &initramfs_data,
            load_result.kernel_end,
            ram_bytes,
            guest_mem,
        )?;
        set_initramfs_params(
            &mut boot_params,
            initramfs_result.gpa,
            initramfs_result.size,
        )?;
    }

    guest_mem.write_slice(Gpa::new(BOOT_PARAMS_GPA), boot_params.as_bytes())?;

    if config.cpus > 1 {
        let rsdp = build_rsdp();
        guest_mem.write_slice(Gpa::new(RSDP_GPA), &rsdp)?;

        let xsdt = build_xsdt();
        guest_mem.write_slice(Gpa::new(hitz_boot::XSDT_GPA), &xsdt)?;

        let madt = build_madt(config.cpus)?;
        guest_mem.write_slice(Gpa::new(hitz_boot::MADT_GPA), &madt)?;

        set_acpi_rsdp(&mut boot_params, RSDP_GPA);
        guest_mem.write_slice(Gpa::new(BOOT_PARAMS_GPA), boot_params.as_bytes())?;
    }

    Ok((pml4_gpa, load_result))
}

#[allow(clippy::needless_pass_by_ref_mut)]
fn build_kernel_cmdline(
    guest_mem: &mut GuestMemory,
    config: &VmConfig,
    extras: &BootExtras,
) -> Result<(), VmError> {
    let mut cmdline = config.effective_cmdline().to_string();

    if config.net.is_some() {
        let net_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
        let _ = write!(
            cmdline,
            " virtio_mmio.device=0x{VIRTIO_MMIO_SIZE:x}@0x{net_base:x}:{VIRTIO_IRQ_NET}"
        );
    }

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

    if extras.vsock_channels.is_some() {
        let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        let _ = write!(
            cmdline,
            " virtio_mmio.device=0x{VIRTIO_MMIO_SIZE:x}@0x{vsock_base:x}:{VIRTIO_IRQ_VSOCK}"
        );
    }

    cmdline.push('\0');
    guest_mem.write_slice(Gpa::new(CMDLINE_GPA), cmdline.as_bytes())?;
    Ok(())
}

fn setup_devices(
    mmio_bus: &mut MmioBus,
    config: &VmConfig,
    extras: &mut BootExtras,
    guest_mem_arc: &Arc<GuestMemory>,
) -> Result<Option<hitz_net::NetIoHandle>, VmError> {
    if let Some(ref disk_path) = config.disk_path {
        let disk_file = fs::File::open(disk_path)?;
        let block_dev = VirtioBlockDevice::new(disk_file)?;
        let mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let transport = VirtioMmioTransport::new(block_dev, mem, VIRTIO_IRQ_BASE);
        mmio_bus.register(VIRTIO_MMIO_BASE, VIRTIO_MMIO_SIZE, Box::new(transport));
    }

    let net_io_handle = if let Some(ref net_cfg) = config.net {
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

        let adapter_name = net_cfg.adapter_name.as_deref().unwrap_or("hitz-net");
        let handle = hitz_net::start_net_io(
            adapter_name,
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

    if let Some((rx_receiver, tx_sender)) = extras.vsock_channels.take() {
        let vsock_dev = VirtioVsockDevice::with_channels(config.guest_cid, rx_receiver, tx_sender);
        let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        let vsock_mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let vsock_transport = VirtioMmioTransport::new(vsock_dev, vsock_mem, VIRTIO_IRQ_VSOCK);
        mmio_bus.register(vsock_base, VIRTIO_MMIO_SIZE, Box::new(vsock_transport));
    }

    Ok(net_io_handle)
}

#[allow(clippy::expect_used)]
fn run_single_vcpu<H: Hypervisor>(
    mut vcpu: <H::Partition as Partition>::Vcpu,
    devices: &Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: &Arc<GuestMemory>,
    stop_flag: &Arc<AtomicBool>,
) -> Result<ExitReason, VmError> {
    let cancel_handle = vcpu.cancel_handle();
    let stop_clone = Arc::clone(stop_flag);
    let watchdog = std::thread::Builder::new()
        .name("cancel-watchdog".into())
        .spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = <<H::Partition as Partition>::Vcpu as Vcpu>::cancel_via(&cancel_handle);
        })
        .expect("spawn watchdog thread");

    let exit_reason = run_loop::run_vcpu_loop(&mut vcpu, devices, &**guest_mem_arc, stop_flag)?;

    stop_flag.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    Ok(exit_reason)
}

#[allow(clippy::expect_used)]
fn run_multi_vcpu<H: Hypervisor>(
    vcpus: Vec<<H::Partition as Partition>::Vcpu>,
    devices: &Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: &Arc<GuestMemory>,
    stop_flag: &Arc<AtomicBool>,
) -> ExitReason {
    let shared_handles: Arc<[_]> = vcpus.iter().map(Vcpu::cancel_handle).collect();
    let (exit_tx, exit_rx) = mpsc::channel::<Result<ExitReason, hitz_hal::HalError>>();
    let num_vcpus = vcpus.len();

    let watchdog = {
        let stop_clone = Arc::clone(stop_flag);
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
            let devs = Arc::clone(devices);
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

                    match &result {
                        Ok(
                            Ok(ExitReason::Halt | ExitReason::Shutdown | ExitReason::Unexpected(_))
                            | Err(_),
                        )
                        | Err(_) => {
                            stop.store(true, Ordering::Relaxed);
                            cancel_all_vcpus::<<H::Partition as Partition>::Vcpu>(&cancel_handles);
                        }
                        Ok(Ok(ExitReason::Canceled)) => {}
                    }

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

    drop(exit_tx);

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

    stop_flag.store(true, Ordering::Relaxed);
    let _ = watchdog.join();

    for handle in handles {
        let _ = handle.join();
    }

    final_reason
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
    fn validate_config_path_traversal() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let valid_path = tmp.path().to_path_buf();

        let mut config = valid_config(valid_path.clone());
        config.kernel_path = std::path::PathBuf::from("/etc/../shadow");

        let err = validate_config(&config).expect_err("should reject path traversal");
        assert!(err.to_string().contains("path traversal detected"));

        // Legitimate filenames with two dots should pass
        let mut config2 = config;
        config2.kernel_path = std::path::PathBuf::from("vmlinux-5.15..1");
        // Since we didn't touch exists() we just want to ensure it doesn't fail with path traversal
        let err2 =
            validate_config(&config2).expect_err("should reject not found but not traversal");
        assert!(!err2.to_string().contains("path traversal detected"));
    }

    #[test]
    fn havoc_validate_config_guest_agent_traversal() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let valid_path = tmp.path().to_path_buf();

        let mut config = valid_config(valid_path.clone());
        config.guest_agent =
            hitz_api::GuestAgentMode::Custom(std::path::PathBuf::from("../../etc/passwd"));

        let err =
            validate_config(&config).expect_err("should reject path traversal in guest agent");
        assert!(err.to_string().contains("path traversal detected"));

        // Also test absolute path
        let mut config_abs = valid_config(valid_path);
        config_abs.guest_agent =
            hitz_api::GuestAgentMode::Custom(std::path::PathBuf::from("/etc/passwd"));

        let err_abs =
            validate_config(&config_abs).expect_err("should reject absolute path in guest agent");
        assert!(err_abs.to_string().contains("absolute paths are not allowed"));
    }
    #[test]
    fn validate_config_table_driven() {
        struct TestCase {
            name: &'static str,
            config: VmConfig,
            expected_error: Option<&'static str>,
        }

        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let valid_path = tmp.path().to_path_buf();

        let cases = vec![
            TestCase {
                name: "Valid minimal config",
                config: valid_config(valid_path.clone()),
                expected_error: None,
            },
            TestCase {
                name: "Missing kernel",
                config: valid_config("nonexistent_kernel".into()),
                expected_error: Some("kernel not found"),
            },
            TestCase {
                name: "Missing initramfs",
                config: {
                    let mut c = valid_config(valid_path.clone());
                    c.initramfs_path = Some("nonexistent_initramfs.cpio".into());
                    c
                },
                expected_error: Some("initramfs not found"),
            },
            TestCase {
                name: "Missing disk",
                config: {
                    let mut c = valid_config(valid_path.clone());
                    c.disk_path = Some("nonexistent_disk.img".into());
                    c
                },
                expected_error: Some("disk image not found"),
            },
            TestCase {
                name: "RAM too small",
                config: {
                    let mut c = valid_config(valid_path.clone());
                    c.ram_mib = 1;
                    c
                },
                expected_error: Some("at least 2 MiB"),
            },
            TestCase {
                name: "Zero CPUs",
                config: {
                    let mut c = valid_config(valid_path.clone());
                    c.cpus = 0;
                    c
                },
                expected_error: Some("cpus must be"),
            },
            TestCase {
                name: "Too many CPUs",
                config: {
                    let mut c = valid_config(valid_path);
                    c.cpus = 256;
                    c
                },
                expected_error: Some("cpus must be"),
            },
        ];

        for case in cases {
            let result = validate_config(&case.config);
            match case.expected_error {
                Some(err_msg) => {
                    if let Err(err) = result {
                        assert!(
                            err.to_string().contains(err_msg),
                            "{}: unexpected error message: {}",
                            case.name,
                            err
                        );
                    } else {
                        panic!("{} should have failed", case.name);
                    }
                }
                None => {
                    result.unwrap_or_else(|_| panic!("{} should have succeeded", case.name));
                }
            }
        }
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
