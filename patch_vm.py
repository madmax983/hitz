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

# Replace expect
code = code.replace(""".expect("spawn watchdog thread")""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))?""")
code = code.replace(""".expect("spawn watchdog thread");""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))?;""")
code = code.replace(""".expect("spawn vcpu thread")""", """.map_err(|e| VmError::ThreadSpawn(e.to_string()))?""")

# Replace run_single_vcpu and run_multi_vcpu parameters
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

# Change guest_mem: &mut GuestMemory to guest_mem: &GuestMemory
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

# Fix needless_borrow
code = code.replace("Arc::clone(&stop_flag)", "Arc::clone(stop_flag)")
code = code.replace("Arc::clone(&devices)", "Arc::clone(devices)")
code = code.replace("Arc::clone(&guest_mem_arc)", "Arc::clone(guest_mem_arc)")
code = code.replace("run_loop::run_vcpu_loop(&mut vcpu, &devices, &*guest_mem_arc, &stop_flag)?;", "run_loop::run_vcpu_loop(&mut vcpu, devices, &**guest_mem_arc, stop_flag)?;")

# Fix `map` with `Result` inside `handles` collection
code = code.replace("""let handles: Vec<_> = vcpus""", """let handles: Vec<_> = vcpus""")
code = code.replace(""".collect();""", """.collect::<Result<Vec<_>, _>>()?;""")

# Fix join
code = code.replace("""let _ = watchdog.join();""", """let _ = watchdog?.join();""")
code = code.replace("""let _ = handle.join();""", """let _ = handle?.join();""")

with open("crates/hitz-vmm/src/vm.rs", "w") as f:
    f.write(code)
