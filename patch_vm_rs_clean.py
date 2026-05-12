import re

with open("crates/hitz-vmm/src/vm.rs", "r") as f:
    code = f.read()

# Add ThreadSpawn to VmError
code = code.replace("""pub enum VmError {
    /// I/O error (reading kernel, initramfs, disk).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),""", """pub enum VmError {
    /// I/O error (reading kernel, initramfs, disk).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Thread spawn failed.
    #[error("thread spawn failed: {0}")]
    ThreadSpawn(String),""")

# Replace .expect on thread spawns
code = code.replace(""".expect("spawn watchdog thread")""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))?""")
code = code.replace(""".expect("spawn vcpu thread")""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))?""")

# Replace Arc parameters
code = code.replace("""fn run_single_vcpu<H: Hypervisor>(
    mut vcpu: <H::Partition as Partition>::Vcpu,
    devices: Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: Arc<GuestMemory>,
    stop_flag: Arc<AtomicBool>,
)""", """fn run_single_vcpu<H: Hypervisor>(
    mut vcpu: <H::Partition as Partition>::Vcpu,
    devices: &Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: &Arc<GuestMemory>,
    stop_flag: &Arc<AtomicBool>,
)""")

code = code.replace("""fn run_multi_vcpu<H: Hypervisor>(
    vcpus: Vec<<H::Partition as Partition>::Vcpu>,
    devices: Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: Arc<GuestMemory>,
    stop_flag: Arc<AtomicBool>,
)""", """fn run_multi_vcpu<H: Hypervisor>(
    vcpus: Vec<<H::Partition as Partition>::Vcpu>,
    devices: &Arc<Mutex<SharedDevices<impl Write + Send + 'static>>>,
    guest_mem_arc: &Arc<GuestMemory>,
    stop_flag: &Arc<AtomicBool>,
)""")

code = code.replace("""run_single_vcpu::<H>(single_vcpu, devices, guest_mem_arc, stop_flag)?""", """run_single_vcpu::<H>(single_vcpu, &devices, &guest_mem_arc, &stop_flag)?""")
code = code.replace("""run_multi_vcpu::<H>(vcpus, devices, guest_mem_arc, stop_flag)?""", """run_multi_vcpu::<H>(vcpus, &devices, &guest_mem_arc, &stop_flag)?""")

# Fix references in run_single_vcpu
code = code.replace("Arc::clone(&stop_flag)", "Arc::clone(stop_flag)")
code = code.replace("Arc::clone(&devices)", "Arc::clone(devices)")
code = code.replace("Arc::clone(&guest_mem_arc)", "Arc::clone(guest_mem_arc)")
code = code.replace("run_loop::run_vcpu_loop(&mut vcpu, &devices, &*guest_mem_arc, &stop_flag)?;", "run_loop::run_vcpu_loop(&mut vcpu, devices, &**guest_mem_arc, stop_flag)?;")

# Fix needless_mut
code = code.replace("""fn build_kernel_cmdline(
    guest_mem: &mut GuestMemory,
    config: &VmConfig,
    extras: &BootExtras,
)""", """fn build_kernel_cmdline(
    guest_mem: &GuestMemory,
    config: &VmConfig,
    extras: &BootExtras,
)""")
code = code.replace("build_kernel_cmdline(&mut guest_mem, config, &extras)?;", "build_kernel_cmdline(&guest_mem, config, &extras)?;")

# Change std::io::Error::new to std::io::Error::other
code = code.replace("""std::io::Error::new(
                std::io::ErrorKind::Other,
                "vcpus array is unexpectedly empty when it should have 1 element",
            )""", """std::io::Error::other("vcpus array is unexpectedly empty when it should have 1 element")""")

# Note: The `map` inside `run_multi_vcpu` now has a closure that returns `Result<JoinHandle, VmError>`, so we must collect it as `Result<Vec<_>, VmError>`:
code = code.replace("let handles: Vec<_> = vcpus", "let handles: Vec<_> = vcpus")
code = code.replace(".collect();", ".collect::<Result<Vec<_>, VmError>>()?;")

with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(code)
