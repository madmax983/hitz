with open("crates/hitz-vmm/src/vm.rs") as f:
    content = f.read()

content = content.replace(
    "pub fn boot_and_run<H: Hypervisor, W: Write + Send + 'static>(\n    hypervisor: &H,\n    config: &VmConfig,\n    serial_out: W,\n    stop_flag: Arc<AtomicBool>,\n    extras: BootExtras,\n)",
    "pub fn boot_and_run<H: Hypervisor, W: Write + Send + 'static>(\n    hypervisor: &H,\n    config: &VmConfig,\n    serial_out: W,\n    stop_flag: Arc<AtomicBool>,\n    mut extras: BootExtras,\n)"
)

content = content.replace("build_kernel_cmdline(&mut guest_mem, config, &extras)?;", "build_kernel_cmdline(&mut guest_mem, config, &extras)?;")
content = content.replace("let net_io_handle = setup_devices(&mut mmio_bus, config, &extras, &guest_mem_arc)?;", "let net_io_handle = setup_devices(&mut mmio_bus, config, &mut extras, &guest_mem_arc)?;")


content = content.replace(
"""fn setup_devices(
    mmio_bus: &mut MmioBus,
    config: &VmConfig,
    extras: &BootExtras,
    guest_mem_arc: &Arc<GuestMemory>,
) -> Result<Option<hitz_net::NetIoHandle>, VmError> {""",
"""fn setup_devices(
    mmio_bus: &mut MmioBus,
    config: &VmConfig,
    extras: &mut BootExtras,
    guest_mem_arc: &Arc<GuestMemory>,
) -> Result<Option<hitz_net::NetIoHandle>, VmError> {""")

content = content.replace(
"""    if let Some((rx_receiver, tx_sender)) = extras.vsock_channels.as_ref() {
        let vsock_dev = VirtioVsockDevice::with_channels(
            config.guest_cid,
            rx_receiver.clone(),
            tx_sender.clone(),
        );""",
"""    if let Some((rx_receiver, tx_sender)) = extras.vsock_channels.take() {
        let vsock_dev = VirtioVsockDevice::with_channels(
            config.guest_cid,
            rx_receiver,
            tx_sender,
        );""")


with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(content)
