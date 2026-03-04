//! Reusable VM boot pipeline.
//!
//! Extracts the 17-step boot sequence from integration tests into a single
//! `boot_and_run` function that can be called from the CLI, daemon, or tests.

use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use hitz_api::VmConfig;
use hitz_boot::{
    BOOT_PARAMS_GPA, CMDLINE_GPA, build_boot_params, build_page_tables, load_elf, load_initramfs,
    set_initramfs_params,
};
use hitz_devices::mmio_bus::MmioBus;
use hitz_devices::serial::SerialDevice;
use hitz_devices::virtio::block::VirtioBlockDevice;
use hitz_devices::virtio::mmio_transport::VirtioMmioTransport;
use hitz_devices::virtio::net::VirtioNetDevice;
use hitz_hal::{
    Gpa, GuestMemAccess, Hypervisor, MemFlags, MemSizeMiB, Partition, PartitionConfig, VcpuId,
};
use hitz_net::ethernet;

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

/// Minimum RAM in MiB (kernel + page tables + `boot_params` need at least 2 MiB).
const MIN_RAM_MIB: u32 = 2;

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
    Ok(())
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
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub fn boot_and_run<H: Hypervisor, W: Write>(
    hypervisor: &H,
    config: &VmConfig,
    serial_out: W,
    stop_flag: Option<&AtomicBool>,
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
        let (guest_ip, _) = ethernet::parse_cidr(&net_cfg.guest_ip).map_err(VmError::Config)?;
        let (gateway_ip, _) = ethernet::parse_cidr(&net_cfg.host_ip).map_err(VmError::Config)?;
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

    let mut cmdline_bytes = cmdline.as_bytes().to_vec();
    cmdline_bytes.push(0); // null-terminate
    guest_mem.write_slice(Gpa::new(CMDLINE_GPA), &cmdline_bytes)?;

    // ── 9. Write GDT ──
    boot_regs::write_gdt(&guest_mem)?;

    // ── 10. Create partition + map memory ──
    let partition_cfg = PartitionConfig {
        vcpu_count: 1,
        memory_size: MemSizeMiB::new(u64::from(config.ram_mib)),
    };
    let mut partition = hypervisor.create_partition(&partition_cfg)?;
    guest_mem.map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)?;

    // ── 11. Create vCPU + configure registers ──
    let mut vcpu = partition.create_vcpu(VcpuId::new(0))?;
    boot_regs::configure_sregs(&mut vcpu, pml4_gpa)?;
    boot_regs::configure_regs(
        &mut vcpu,
        load_result.entry_point,
        Gpa::new(BOOT_PARAMS_GPA),
    )?;

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
            ethernet::parse_mac(mac_str).map_err(VmError::Config)?
        } else {
            ethernet::random_mac()
        };

        let gateway_mac: [u8; 6] = [0xAA, 0xBB, 0xCC, 0x00, 0x00, 0x01];
        let (gateway_ip, _) = ethernet::parse_cidr(&net_cfg.host_ip).map_err(VmError::Config)?;

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

    // ── 14. Run vCPU loop ──
    let devices = Mutex::new(SharedDevices { serial, mmio_bus });

    // Create a local stop flag if the caller didn't provide one.
    let local_stop = AtomicBool::new(false);
    let effective_stop = stop_flag.unwrap_or(&local_stop);

    let exit_reason =
        run_loop::run_vcpu_loop(&mut vcpu, &devices, &*guest_mem_arc, effective_stop)?;

    // Explicitly drop the net I/O handle after the run loop exits.
    // This signals the I/O thread to stop and joins it.
    drop(net_io_handle);

    Ok(VmRunResult { exit_reason })
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
        }
    }

    #[test]
    fn validate_config_missing_kernel() {
        let cfg = valid_config("nonexistent_kernel".into());
        let err = validate_config(&cfg).unwrap_err();
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
        let err = validate_config(&cfg).unwrap_err();
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
        let err = validate_config(&cfg).unwrap_err();
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
        let err = validate_config(&cfg).unwrap_err();
        assert!(
            err.to_string().contains("at least 2 MiB"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_config_ok() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let cfg = valid_config(tmp.path().to_path_buf());
        validate_config(&cfg).expect("valid config should pass");
    }
}
