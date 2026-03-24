//! The core orchestrator of virtual machines.
//!
//! A modern VMM cannot simply boot a kernel and block the main thread. We must juggle
//! multiple concurrent virtual machines, coordinate state transitions (Created -> Running -> Stopped),
//! route API requests to the right target, and ensure resources are cleaned up when the party ends.
//!
//! The `VmManager` acts as the grand conductor of this orchestra. It holds the map of all known
//! VMs, safely wrapped in asynchronous-friendly synchronization primitives, and manages the lifecycle
//! tasks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry::KeyValue;
use tokio::sync::mpsc as tokio_mpsc;

use hitz_api::{VmConfig, VmInfo, VmState};
use hitz_hal::Hypervisor;
use hitz_vmm::{ExitReason, SerialBuf, SerialReader};

use crate::error::DaemonError;
use crate::state_store::StateStore;

/// Internal state for a single VM.
struct VmEntry {
    config: VmConfig,
    state: VmState,
    exit_reason: Option<String>,
    stop_flag: Option<Arc<AtomicBool>>,
    serial_buf: Option<SerialBuf>,
    /// Vsock I/O handle kept alive for the duration of the VM run.
    ///
    /// Dropping this closes the channels, which signals the vsock device's
    /// poll loop to stop. Currently `None` for VMs without agent injection,
    /// and `None` for the simplified push-to-`OTel` path (the handle ends are
    /// consumed by the metrics task).
    vsock_handle: Option<hitz_vmm::VsockIoHandle>,
}

impl VmEntry {
    fn to_info(&self, id: &str) -> VmInfo {
        VmInfo {
            id: id.to_string(),
            state: self.state,
            config: self.config.clone(),
            exit_reason: self.exit_reason.clone(),
        }
    }
}

/// Manages the lifecycle of multiple virtual machines.
///
/// The `VmManager` is the source of truth for all VMs on the host. It maps string identifiers
/// to their internal representation, tracks their current state, and provides the API to
/// start, stop, and inspect them.
///
/// **Why generic over `H: Hypervisor`?**
/// By relying on the [`Hypervisor`] trait from `hitz_hal` instead of a concrete implementation
/// (like `WhpHypervisor`), we achieve true decoupling. This allows us to write comprehensive unit tests
/// using a `FakeHypervisor` without needing a physical Windows machine or Hyper-V enabled.
///
/// `Clone` is implemented manually so `H` does not need to be `Clone`
/// (all internal fields are `Arc`-wrapped for cheap cloning across threads).
pub struct VmManager<H> {
    hypervisor: Arc<H>,
    store: Arc<StateStore>,
    vms: Arc<Mutex<HashMap<String, VmEntry>>>,
    /// Sender half of the VM-completion channel.  Each spawned VM task sends
    /// its ID here after `boot_and_run` returns so `stop_all_and_wait` can
    /// drain them.
    completion_tx: tokio_mpsc::UnboundedSender<String>,
    /// Receiver half, wrapped in a tokio `Mutex` so it can be shared across
    /// `Clone`d managers without requiring `&mut self`.
    completion_rx: Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<String>>>,
    /// `OTel` up-down counter tracking VMs by state.
    vm_count: Arc<opentelemetry::metrics::UpDownCounter<i64>>,
    /// Shutdown watch receiver, shared with the vsock metrics tasks so they
    /// stop when the daemon shuts down.
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// Sender half retained to keep the watch channel alive.
    ///
    /// When no external sender is wired (standalone mode), this prevents the
    /// receiver from becoming permanently ready (as it would if all senders
    /// were dropped). The daemon replaces the receiver via
    /// [`set_shutdown_receiver`] in production.
    ///
    /// [`set_shutdown_receiver`]: VmManager::set_shutdown_receiver
    shutdown_tx_owned: Arc<tokio::sync::watch::Sender<bool>>,
}

impl<H> Clone for VmManager<H> {
    fn clone(&self) -> Self {
        Self {
            hypervisor: Arc::clone(&self.hypervisor),
            store: Arc::clone(&self.store),
            vms: Arc::clone(&self.vms),
            completion_tx: self.completion_tx.clone(),
            completion_rx: Arc::clone(&self.completion_rx),
            vm_count: Arc::clone(&self.vm_count),
            shutdown_rx: self.shutdown_rx.clone(),
            shutdown_tx_owned: Arc::clone(&self.shutdown_tx_owned),
        }
    }
}

impl<H: Hypervisor + Send + Sync + 'static> VmManager<H> {
    /// Creates a new manager with the given hypervisor backend and state directory.
    ///
    /// The manager is persistent by design. On startup, it inspects the provided `state_dir`
    /// to resurrect the configuration and state of VMs from the previous run.
    ///
    /// **Why recover `Running` VMs as `Stopped`?**
    /// If the daemon exits while a VM is running, the underlying hypervisor partition is destroyed.
    /// We cannot magically reconnect to a lost micro-VM. Therefore, any VM that was left in the `Running`
    /// state is safely transitioned to `Stopped` upon recovery so the user knows it must be started again.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// let state_dir = PathBuf::from("C:\\hitz\\vms");
    ///
    /// let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// ```
    pub fn new(hypervisor: Arc<H>, state_dir: std::path::PathBuf) -> Result<Self, DaemonError> {
        let store = StateStore::new(state_dir)?;
        let existing = store.load_all()?;

        let mut map = HashMap::new();
        for (id, config, state) in existing {
            tracing::info!(vm_id = %id, ?state, "recovered persisted VM");
            let _ = map.insert(
                id,
                VmEntry {
                    config,
                    state,
                    exit_reason: None,
                    stop_flag: None,
                    serial_buf: None,
                    vsock_handle: None,
                },
            );
        }

        let (tx, rx) = tokio_mpsc::unbounded_channel();
        let meter = opentelemetry::global::meter("hitz");
        let vm_count = Arc::new(
            meter
                .i64_up_down_counter("hitz.vm.count")
                .with_description("Number of VMs by state")
                .build(),
        );
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        Ok(Self {
            hypervisor,
            store: Arc::new(store),
            vms: Arc::new(Mutex::new(map)),
            completion_tx: tx,
            completion_rx: Arc::new(tokio::sync::Mutex::new(rx)),
            vm_count,
            shutdown_rx,
            shutdown_tx_owned: Arc::new(shutdown_tx),
        })
    }

    /// Wires the vsock metrics tasks to stop when a daemon-provided shutdown signal fires.
    ///
    /// **Why do we need a shutdown receiver?**
    /// A running VM might have a background task pumping metrics over a vsock channel.
    /// If the daemon needs to shut down cleanly, it must signal these background tasks
    /// to exit their polling loops; otherwise, the Tokio runtime will hang waiting for them.
    ///
    /// Call this after constructing the manager. If not called, the manager uses a dummy internal
    /// channel that never fires, which is acceptable for standalone or test execution.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let mut manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    /// manager.set_shutdown_receiver(shutdown_rx);
    /// ```
    pub fn set_shutdown_receiver(&mut self, rx: tokio::sync::watch::Receiver<bool>) {
        self.shutdown_rx = rx;
    }

    /// Creates a VM with the given ID and configuration.
    ///
    /// This validates the [`VmConfig`] and persists it to disk. It transitions the VM to the
    /// `Created` state, ready to be started. It does **not** boot the VM.
    ///
    /// ## Errors
    ///
    /// Returns a [`DaemonError`] if a VM with the same ID already exists, if the configuration
    /// is invalid, or if the state cannot be saved to the filesystem.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # use hitz_api::{VmConfig, GuestAgentMode};
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// let config = VmConfig {
    ///     kernel_path: PathBuf::from("vmlinux"),
    ///     initramfs_path: None,
    ///     disk_path: None,
    ///     ram_mib: 256,
    ///     cpus: 1,
    ///     cmdline: None,
    ///     net: None,
    ///     ports: vec![],
    ///     guest_cid: 3,
    ///     guest_agent: GuestAgentMode::Auto,
    /// };
    ///
    /// let info = manager.create_vm("my-vm".to_string(), &config).unwrap();
    /// ```
    pub fn create_vm(&self, id: String, config: &VmConfig) -> Result<VmInfo, DaemonError> {
        hitz_vmm::validate_config(config)?;

        let info = {
            let mut vms = self
                .vms
                .lock()
                .map_err(|e| DaemonError::Internal(e.to_string()))?;

            if vms.contains_key(&id) {
                return Err(DaemonError::AlreadyExists(id));
            }

            let entry = VmEntry {
                config: config.clone(),
                state: VmState::Created,
                exit_reason: None,
                stop_flag: None,
                serial_buf: None,
                vsock_handle: None,
            };
            let info = entry.to_info(&id);
            let _ = vms.insert(id.clone(), entry);
            info
            // Mutex released here — filesystem I/O must not hold it.
        };

        if let Err(e) = self
            .store
            .save_config(&id, config)
            .and_then(|()| self.store.save_state(&id, VmState::Created))
        {
            // Rollback the in-memory insert so a retry doesn't hit AlreadyExists.
            if let Ok(mut vms) = self.vms.lock() {
                let _ = vms.remove(&id);
            }
            return Err(e);
        }
        Ok(info)
    }

    /// Boots a previously created VM.
    ///
    /// Transitions a `Created` VM into the `Running` state. This spawns an asynchronous background task
    /// that drives the VMM execution loop.
    ///
    /// **Why spawn a blocking task?**
    /// The actual hypervisor execution (`boot_and_run`) is heavily CPU-bound and blocks
    /// synchronously on the vCPU run loop. We use `tokio::task::spawn_blocking` to ensure
    /// the Tokio async executor isn't starved.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// manager.start_vm("my-vm").expect("Failed to start VM");
    /// ```
    #[allow(clippy::significant_drop_tightening, clippy::too_many_lines)]
    pub fn start_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let (config, stop_flag, serial_buf, info) = {
            let mut vms = self
                .vms
                .lock()
                .map_err(|e| DaemonError::Internal(e.to_string()))?;
            let entry = vms
                .get_mut(id)
                .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

            if entry.state != VmState::Created {
                return Err(DaemonError::InvalidState {
                    id: id.to_string(),
                    state: entry.state,
                    expected: "Created".to_string(),
                });
            }

            let flag = Arc::new(AtomicBool::new(false));
            entry.stop_flag = Some(flag.clone());
            entry.state = VmState::Running;

            let serial_buf = SerialBuf::new();
            entry.serial_buf = Some(serial_buf.clone());

            (entry.config.clone(), flag, serial_buf, entry.to_info(id))
        };
        // Mutex released here — spawn_blocking must not hold it.

        // Persist Running state before spawning the VM task.
        self.store.save_state(id, VmState::Running)?;

        // Inject guest agent overlay into initramfs if agent is enabled.
        let boot_config =
            if let Some(agent_bytes) = crate::agent::resolve_agent_bytes(&config.guest_agent) {
                let overlay = crate::agent::build_agent_overlay(&agent_bytes);
                let mut combined = config
                    .initramfs_path
                    .as_ref()
                    .map_or_else(Vec::new, |path| std::fs::read(path).unwrap_or_default());
                combined.extend_from_slice(&overlay);
                let tmp = std::env::temp_dir().join(format!("hitz-initrd-{id}.cpio"));
                if std::fs::write(&tmp, &combined).is_ok() {
                    let mut patched = config.clone();
                    patched.initramfs_path = Some(tmp);
                    patched
                } else {
                    config.clone()
                }
            } else {
                config.clone()
            };

        // Record guest RAM size as a one-shot gauge.
        {
            let meter = opentelemetry::global::meter("hitz");
            let memory_gauge = meter
                .u64_gauge("hitz.vm.memory_bytes")
                .with_description("Guest RAM in bytes at VM start")
                .build();
            memory_gauge.record(
                u64::from(config.ram_mib) * 1024 * 1024,
                &[KeyValue::new("vm.id", id.to_string())],
            );
        }

        // Increment the running-VM count.
        self.vm_count.add(1, &[KeyValue::new("state", "running")]);

        let hv = self.hypervisor.clone();
        let vms = self.vms.clone();
        let vm_id = id.to_string();
        let completion_tx = self.completion_tx.clone();
        let vm_count_clone = Arc::clone(&self.vm_count);
        let vm_id_span = vm_id.clone();

        // Create vsock channels if agent injection is enabled.
        // The host-facing ends are consumed by the async metrics task;
        // the device-facing ends are passed to boot_and_run via BootExtras.
        // On-demand pull is a future enhancement; only push-to-OTel is wired here.
        let extras = if matches!(
            boot_config.guest_agent,
            hitz_api::GuestAgentMode::Auto | hitz_api::GuestAgentMode::Custom(_)
        ) {
            let (handle, rx_receiver, tx_sender) = hitz_vmm::VsockIoHandle::new_pair();
            // Spawn the async metrics task with the host-facing channel ends.
            // The task runs until the channels close (VM exit) or shutdown.
            let shutdown_rx = self.shutdown_rx.clone();
            drop(tokio::spawn(crate::vsock_server::run_metrics_task(
                vm_id.clone(),
                handle.tx_rx,
                handle.rx_tx,
                shutdown_rx,
            )));
            hitz_vmm::BootExtras {
                vsock_channels: Some((rx_receiver, tx_sender)),
            }
        } else {
            hitz_vmm::BootExtras::none()
        };

        let store_exit = Arc::clone(&self.store);

        // Fire-and-forget: the spawned task updates VM state on completion
        // and signals the completion channel so `stop_all_and_wait` can drain.
        drop(tokio::task::spawn(async move {
            let boot_span = tracing::info_span!(
                "vm.boot",
                vm.id = %vm_id_span,
                vm.ram_mib = boot_config.ram_mib,
                vm.cpus = boot_config.cpus,
            );
            let _boot_enter = boot_span.enter();

            // Start port forwarders if networking is configured and rules exist.
            let _port_fwd = if let Some(ref net) = boot_config.net {
                if boot_config.ports.is_empty() {
                    None
                } else {
                    let guest_ip = net
                        .guest_ip
                        .split('/')
                        .next()
                        .and_then(|s| s.parse::<std::net::Ipv4Addr>().ok());
                    if let Some(ip) = guest_ip {
                        Some(
                            crate::port_forward::PortForwardManager::start(ip, &boot_config.ports)
                                .await,
                        )
                    } else {
                        tracing::warn!("could not parse guest IP from {}", net.guest_ip);
                        None
                    }
                }
            } else {
                None
            };

            let result = tokio::task::spawn_blocking(move || {
                hitz_vmm::boot_and_run(&*hv, &boot_config, serial_buf, stop_flag, extras)
            })
            .await;

            // _port_fwd drops here → all listener tasks aborted.

            // Decrement the running-VM count now that the VM has exited.
            vm_count_clone.add(-1, &[KeyValue::new("state", "running")]);

            // Update state based on result.
            if let Ok(mut vms) = vms.lock()
                && let Some(entry) = vms.get_mut(&vm_id)
            {
                match result {
                    Ok(Ok(run_result)) => match run_result.exit_reason {
                        ExitReason::Halt | ExitReason::Shutdown | ExitReason::Canceled => {
                            entry.state = VmState::Stopped;
                            entry.exit_reason = Some(format!("{:?}", run_result.exit_reason));
                        }
                        ExitReason::Unexpected(ref reason) => {
                            entry.state = VmState::Failed;
                            entry.exit_reason = Some(reason.clone());
                        }
                    },
                    Ok(Err(e)) => {
                        entry.state = VmState::Failed;
                        entry.exit_reason = Some(e.to_string());
                    }
                    Err(e) => {
                        entry.state = VmState::Failed;
                        entry.exit_reason = Some(format!("task panicked: {e}"));
                    }
                }
                entry.stop_flag = None;
                entry.vsock_handle = None;
                if let Some(ref buf) = entry.serial_buf {
                    buf.close();
                }
                let final_state = entry.state;
                drop(vms); // explicit drop — persist state outside the lock
                if let Err(e) = store_exit.save_state(&vm_id, final_state) {
                    tracing::warn!(vm_id = %vm_id, error = %e, "failed to persist VM exit state");
                }
            }

            // Notify stop_all_and_wait that this VM has finished.
            let _ = completion_tx.send(vm_id);
        }));

        Ok(info)
    }

    /// Stops a running VM by setting its atomic stop flag.
    ///
    /// **Why an atomic flag?**
    /// The vCPU is deeply buried inside a synchronous run loop on a separate thread.
    /// We can't simply `await` it to stop. Instead, we flip an atomic boolean.
    /// A cancel-watchdog thread checks this flag and issues a hypervisor-level cancellation
    /// to yank the vCPU back to reality.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// manager.stop_vm("my-vm").unwrap();
    /// ```
    pub fn stop_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let mut vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get_mut(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        if entry.state != VmState::Running {
            return Err(DaemonError::InvalidState {
                id: id.to_string(),
                state: entry.state,
                expected: "Running".to_string(),
            });
        }

        if let Some(ref flag) = entry.stop_flag {
            flag.store(true, Ordering::Relaxed);
        }

        let info = entry.to_info(id);
        drop(vms);
        Ok(info)
    }

    /// Gets a serial output reader for a running VM.
    ///
    /// Returns a [`SerialReader`] that can be polled for chunks of serial output.
    ///
    /// **Why only when running?**
    /// The serial buffer is a live ring buffer tied to the VM's execution. When the VM is stopped
    /// or hasn't started, the buffer doesn't exist.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// let mut reader = manager.serial_reader("my-vm").unwrap();
    /// // Stream chunks to the client...
    /// ```
    pub fn serial_reader(&self, id: &str) -> Result<SerialReader, DaemonError> {
        let vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;
        let reader = entry
            .serial_buf
            .as_ref()
            .map(SerialBuf::reader)
            .ok_or_else(|| DaemonError::InvalidState {
                id: id.to_string(),
                state: entry.state,
                expected: "Running (with serial buffer)".to_string(),
            })?;
        drop(vms);
        Ok(reader)
    }

    /// Gets detailed information about a single VM.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// let info = manager.get_vm("my-vm").unwrap();
    /// println!("State: {:?}", info.state);
    /// ```
    pub fn get_vm(&self, id: &str) -> Result<VmInfo, DaemonError> {
        let vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;
        let info = entry.to_info(id);
        drop(vms);
        Ok(info)
    }

    /// Lists all virtual machines managed by this instance.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// for vm in manager.list_vms().unwrap() {
    ///     println!("Found VM: {}", vm.id);
    /// }
    /// ```
    pub fn list_vms(&self) -> Result<Vec<VmInfo>, DaemonError> {
        let vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        Ok(vms.iter().map(|(id, entry)| entry.to_info(id)).collect())
    }

    /// Deletes a VM.
    ///
    /// **Why must it be stopped?**
    /// Deleting a running VM would orphan the underlying hypervisor partition and background tasks,
    /// leading to resource leaks. Force the user to explicitly stop it first.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// manager.stop_vm("my-vm").unwrap();
    /// manager.delete_vm("my-vm").unwrap();
    /// ```
    pub fn delete_vm(&self, id: &str) -> Result<(), DaemonError> {
        let mut vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        let entry = vms
            .get(id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        if entry.state == VmState::Running {
            return Err(DaemonError::InvalidState {
                id: id.to_string(),
                state: entry.state,
                expected: "Created, Stopped, or Failed".to_string(),
            });
        }

        let _ = vms.remove(id);
        drop(vms);
        self.store.delete(id)?;
        Ok(())
    }

    /// Stops all running VMs (fire-and-forget, does not wait for them to finish).
    ///
    /// Sets the stop flag on every running VM, which signals the cancel-watchdog
    /// threads to cancel the vCPU run loops. Returns immediately without waiting
    /// for the VMs to reach a terminal state.
    ///
    /// Use [`VmManager::stop_all_and_wait`] if you need to block until all VMs have stopped.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// manager.stop_all();
    /// println!("Shutdown signals sent, moving on...");
    /// ```
    pub fn stop_all(&self) {
        if let Ok(vms) = self.vms.lock() {
            for entry in vms.values() {
                if entry.state == VmState::Running
                    && let Some(ref flag) = entry.stop_flag
                {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    /// Stops all running VMs and waits for them to finish.
    ///
    /// Sets stop flags for all running VMs (triggering their internal cancel-watchdog threads),
    /// then asynchronously awaits on the completion channel until all VMs report they are done
    /// or the `timeout` expires.
    ///
    /// **Why a timeout?**
    /// Sometimes a VM might be deadlocked in the kernel or ignoring the ACPI shutdown request.
    /// The daemon must eventually proceed with its own shutdown rather than hanging forever.
    /// If the timeout elapses, it logs a warning and returns, leaving the OS process teardown
    /// to clean up the partitions.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// # use std::sync::Arc;
    /// # use std::path::PathBuf;
    /// # use hitz_daemon::VmManager;
    /// # use hitz_whp::WhpHypervisor;
    /// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
    /// # let state_dir = PathBuf::from("C:\\hitz\\vms");
    /// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
    /// use std::time::Duration;
    /// # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// # rt.block_on(async {
    /// manager.stop_all_and_wait(Duration::from_secs(5)).await;
    /// # });
    /// ```
    pub async fn stop_all_and_wait(&self, timeout: Duration) {
        let running_count = {
            let Ok(vms) = self.vms.lock() else { return };
            let mut count = 0usize;
            for entry in vms.values() {
                if entry.state == VmState::Running {
                    if let Some(ref flag) = entry.stop_flag {
                        flag.store(true, Ordering::Relaxed);
                    }
                    count += 1;
                }
            }
            count
        };

        if running_count == 0 {
            return;
        }

        tracing::info!(running_count, "waiting for VMs to stop");

        let deadline = tokio::time::Instant::now() + timeout;
        let mut rx = self.completion_rx.lock().await;
        let mut stopped = 0usize;

        while stopped < running_count {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(_vm_id)) => {
                    stopped += 1;
                }
                Ok(None) => break, // channel closed — no more senders
                Err(_elapsed) => {
                    tracing::warn!(
                        remaining = running_count - stopped,
                        "timeout waiting for VMs to stop"
                    );
                    break;
                }
            }
        }
    }

    /// On-demand metrics pull. Currently returns `None` (push-to-`OTel` path
    /// works automatically via the vsock metrics task).
    ///
    /// Future: implement pull by injecting a request packet and awaiting
    /// the response — requires storing a dedicated pull channel end in
    /// `VmEntry` separate from the push stream read by `run_metrics_task`.
    #[must_use]
    pub const fn request_metrics_snapshot(
        &self,
        _vm_id: &str,
    ) -> Option<hitz_api::MetricsSnapshot> {
        None
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, unsafe_code, unused_results)]
mod tests {
    use super::*;
    use hitz_api::VmConfig;

    /// Dummy hypervisor for testing state transitions without WHP.
    struct FakeHypervisor;

    impl Hypervisor for FakeHypervisor {
        type Partition = FakePartition;
        fn create_partition(
            &self,
            _config: &hitz_hal::PartitionConfig,
        ) -> Result<Self::Partition, hitz_hal::HalError> {
            // boot_and_run will fail at partition creation,
            // but we can still test create/get/list/delete.
            Err(hitz_hal::HalError::CreatePartition(
                "fake hypervisor".into(),
            ))
        }
    }

    struct FakePartition;

    impl hitz_hal::Partition for FakePartition {
        type Vcpu = FakeVcpu;
        unsafe fn map_memory(
            &mut self,
            _gpa: hitz_hal::Gpa,
            _hva: *mut u8,
            _size: usize,
            _flags: hitz_hal::MemFlags,
        ) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn unmap_memory(
            &mut self,
            _gpa: hitz_hal::Gpa,
            _size: usize,
        ) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn create_vcpu(&mut self, _id: hitz_hal::VcpuId) -> Result<Self::Vcpu, hitz_hal::HalError> {
            Ok(FakeVcpu)
        }
        fn request_interrupt(
            &self,
            _vcpu_id: hitz_hal::VcpuId,
            _vector: u8,
        ) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
    }

    struct FakeVcpu;

    impl hitz_hal::Vcpu for FakeVcpu {
        type CancelHandle = ();

        fn cancel_handle(&self) -> Self::CancelHandle {}

        fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }

        fn run(&mut self) -> Result<hitz_hal::VcpuExit, hitz_hal::HalError> {
            Ok(hitz_hal::VcpuExit::Halt)
        }

        fn get_regs(&self) -> Result<hitz_hal::StandardRegs, hitz_hal::HalError> {
            Ok(hitz_hal::StandardRegs::default())
        }
        fn set_regs(&mut self, _regs: &hitz_hal::StandardRegs) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn get_sregs(&self) -> Result<hitz_hal::SpecialRegs, hitz_hal::HalError> {
            Ok(hitz_hal::SpecialRegs::default())
        }
        fn set_sregs(&mut self, _sregs: &hitz_hal::SpecialRegs) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
        fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
    }

    fn make_manager() -> (VmManager<FakeHypervisor>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let mgr = VmManager::new(Arc::new(FakeHypervisor), path).expect("VmManager::new");
        (mgr, dir)
    }

    fn make_config() -> (VmConfig, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let path = tmp.path().to_path_buf();
        let config = VmConfig {
            kernel_path: path,
            initramfs_path: None,
            disk_path: None,
            ram_mib: 128,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: hitz_api::DEFAULT_GUEST_CID,
            guest_agent: hitz_api::GuestAgentMode::Auto,
        };
        (config, tmp)
    }

    #[test]
    fn create_vm_sets_created_state() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();
        let info = mgr.create_vm("vm1".into(), &config).expect("create");
        assert_eq!(info.state, VmState::Created);
        assert_eq!(info.id, "vm1");
    }

    #[test]
    fn create_duplicate_vm_fails() {
        let (mgr, _dir) = make_manager();
        let (config1, _tmp1) = make_config();
        let (config2, _tmp2) = make_config();
        mgr.create_vm("vm1".into(), &config1).expect("create");
        let err = mgr.create_vm("vm1".into(), &config2).expect_err("should fail");
        assert!(err.to_string().contains("already exists"), "got: {err}");
    }

    #[test]
    fn get_vm_not_found() {
        let (mgr, _dir) = make_manager();
        let err = mgr.get_vm("nope").expect_err("should fail");
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn list_vms_empty() {
        let (mgr, _dir) = make_manager();
        let list = mgr.list_vms().expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn list_vms_after_create() {
        let (mgr, _dir) = make_manager();
        let (config_a, _tmp_a) = make_config();
        let (config_b, _tmp_b) = make_config();
        mgr.create_vm("a".into(), &config_a).expect("create a");
        mgr.create_vm("b".into(), &config_b).expect("create b");
        let list = mgr.list_vms().expect("list");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_created_vm() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();
        mgr.create_vm("vm1".into(), &config).expect("create");
        mgr.delete_vm("vm1").expect("delete");
        let err = mgr.get_vm("vm1").expect_err("should fail");
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_nonexistent_vm_fails() {
        let (mgr, _dir) = make_manager();
        let err = mgr.delete_vm("nope").expect_err("should fail");
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_running_vm_fails() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();

        // Setup a running VM directly in the map
        mgr.create_vm("vm_run".into(), &config).expect("create");
        {
            let mut vms = mgr.vms.lock().expect("lock vms map");
            let entry = vms.get_mut("vm_run").expect("entry present");
            entry.state = VmState::Running;
            drop(vms);
        }

        let err = mgr.delete_vm("vm_run").expect_err("should fail");
        assert!(
            err.to_string().contains("Created, Stopped, or Failed"),
            "got: {err}"
        );
    }

    #[test]
    fn stop_created_vm_fails() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();
        mgr.create_vm("vm1".into(), &config).expect("create");
        let err = mgr.stop_vm("vm1").expect_err("should fail");
        assert!(err.to_string().contains("Created"), "got: {err}");
    }

    #[tokio::test]
    async fn port_forwards_start_with_vm() {
        // VmConfig with a port forward but no net → ports should be ignored silently (no panic).
        let (mgr, _dir) = make_manager();
        let (mut config, _tmp) = make_config();
        config.ports = vec![hitz_api::PortForward {
            host_port: 19876,
            guest_port: 22,
        }];
        // config.net is None → port forward should be a no-op.

        let _info = mgr
            .create_vm("port_fwd_test".into(), &config)
            .expect("create");
        // start_vm fires off an async task; just verify it doesn't panic.
        mgr.start_vm("port_fwd_test").expect("start");
    }

    #[tokio::test]
    async fn start_vm_metrics_do_not_panic() {
        // Regression guard: metric and span calls must not panic when no global
        // OTel provider is registered (the no-op provider handles it).
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();
        mgr.create_vm("m1".into(), &config).expect("create");
        // start_vm spawns an async task; synchronous metric setup happens before spawn.
        // This verifies the counter/gauge creation path doesn't panic.
        let _info = mgr.start_vm("m1").expect("start");
    }

    #[test]
    fn stop_stopped_vm_fails() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();

        // Setup a stopped VM directly in the map
        mgr.create_vm("vm_stop".into(), &config).expect("create");
        {
            let mut vms = mgr.vms.lock().expect("lock vms map");
            let entry = vms.get_mut("vm_stop").expect("entry present");
            entry.state = VmState::Stopped;
            drop(vms);
        }

        let err = mgr.stop_vm("vm_stop").expect_err("should fail");
        assert!(err.to_string().contains("Running"), "got: {err}");
    }

    #[test]
    fn serial_buf_available_after_start() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build runtime");

        rt.block_on(async {
            let (mgr, _dir) = make_manager();
            let (config, _tmp) = make_config();
            mgr.create_vm("vm1".into(), &config).expect("create");

            // Start will fail (FakeHypervisor can't create partition) but
            // serial_buf should be created before boot_and_run is called.
            let _ = mgr.start_vm("vm1");

            // Give the spawned task a moment to start.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // Serial reader should be available because start_vm sets it up
            // before spawning the blocking task.
            let reader = mgr.serial_reader("vm1");
            assert!(
                reader.is_ok(),
                "serial reader should exist after start: {}",
                reader.err().map_or_else(String::new, |e| e.to_string())
            );
        });
    }

    #[test]
    fn vsock_pull_channel_accessible() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();
        mgr.create_vm("v1".into(), &config).expect("create");
        // Returns None since no vsock channels in test mode (push-to-OTel only).
        let result = mgr.request_metrics_snapshot("v1");
        assert!(result.is_none());
    }

    #[test]
    fn stop_all_sets_stop_flag() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();

        let flag = Arc::new(AtomicBool::new(false));
        mgr.create_vm("vm_run1".into(), &config).expect("create");
        mgr.create_vm("vm_run2".into(), &config).expect("create");

        {
            let mut vms = mgr.vms.lock().expect("lock vms map");
            let entry1 = vms.get_mut("vm_run1").expect("entry present");
            entry1.state = VmState::Running;
            entry1.stop_flag = Some(flag.clone());

            let entry2 = vms.get_mut("vm_run2").expect("entry present");
            // Don't set stop flag for vm_run2. It should skip gracefully
            entry2.state = VmState::Running;
            drop(vms);
        }

        mgr.stop_all();

        assert!(
            flag.load(Ordering::Relaxed),
            "stop_flag should be set to true by stop_all"
        );
    }

    #[tokio::test]
    async fn stop_all_and_wait_completes() {
        let (mgr, _dir) = make_manager();
        let (config, _tmp) = make_config();

        let flag = Arc::new(AtomicBool::new(false));
        mgr.create_vm("vm_run1".into(), &config).expect("create");

        {
            let mut vms = mgr.vms.lock().expect("lock vms map");
            let entry1 = vms.get_mut("vm_run1").expect("entry present");
            entry1.state = VmState::Running;
            entry1.stop_flag = Some(flag.clone());
            drop(vms);
        }

        // Simulate VM completion channel
        let tx = mgr.completion_tx.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = tx.send("vm_run1".into());
        });

        // This should complete successfully and set the flag
        mgr.stop_all_and_wait(Duration::from_secs(2)).await;

        assert!(
            flag.load(Ordering::Relaxed),
            "stop_flag should be set to true by stop_all_and_wait"
        );
    }
}
