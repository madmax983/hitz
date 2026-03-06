//! VM lifecycle manager — coordinates `boot_and_run` tasks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use hitz_api::{VmConfig, VmInfo, VmState};
use hitz_hal::Hypervisor;
use hitz_vmm::{ExitReason, SerialBuf, SerialReader};

use crate::error::DaemonError;

/// Internal state for a single VM.
struct VmEntry {
    config: VmConfig,
    state: VmState,
    exit_reason: Option<String>,
    stop_flag: Option<Arc<AtomicBool>>,
    serial_buf: Option<SerialBuf>,
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

/// Manages the lifecycle of multiple VMs.
///
/// Generic over the hypervisor backend so the daemon library doesn't
/// depend on a specific platform (WHP, KVM, etc.).
///
/// `Clone` is implemented manually so `H` does not need to be `Clone`
/// (all fields are `Arc`-wrapped).
pub struct VmManager<H> {
    hypervisor: Arc<H>,
    vms: Arc<Mutex<HashMap<String, VmEntry>>>,
    /// Sender half of the VM-completion channel.  Each spawned VM task sends
    /// its ID here after `boot_and_run` returns so `stop_all_and_wait` can
    /// drain them.
    completion_tx: tokio_mpsc::UnboundedSender<String>,
    /// Receiver half, wrapped in a tokio `Mutex` so it can be shared across
    /// `Clone`d managers without requiring `&mut self`.
    completion_rx: Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<String>>>,
}

impl<H> Clone for VmManager<H> {
    fn clone(&self) -> Self {
        Self {
            hypervisor: Arc::clone(&self.hypervisor),
            vms: Arc::clone(&self.vms),
            completion_tx: self.completion_tx.clone(),
            completion_rx: Arc::clone(&self.completion_rx),
        }
    }
}

impl<H: Hypervisor + Send + Sync + 'static> VmManager<H> {
    /// Create a new manager with the given hypervisor backend.
    pub fn new(hypervisor: Arc<H>) -> Self {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        Self {
            hypervisor,
            vms: Arc::new(Mutex::new(HashMap::new())),
            completion_tx: tx,
            completion_rx: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    /// Create a VM with the given ID and config. Validates the config
    /// but does not boot.
    pub fn create_vm(&self, id: String, config: VmConfig) -> Result<VmInfo, DaemonError> {
        hitz_vmm::validate_config(&config)?;

        let mut vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;

        if vms.contains_key(&id) {
            return Err(DaemonError::AlreadyExists(id));
        }

        let entry = VmEntry {
            config,
            state: VmState::Created,
            exit_reason: None,
            stop_flag: None,
            serial_buf: None,
        };
        let info = entry.to_info(&id);
        let _ = vms.insert(id, entry);
        drop(vms);
        Ok(info)
    }

    /// Boot a previously created VM.
    #[allow(clippy::significant_drop_tightening)]
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

        let hv = self.hypervisor.clone();
        let vms = self.vms.clone();
        let vm_id = id.to_string();
        let completion_tx = self.completion_tx.clone();

        // Fire-and-forget: the spawned task updates VM state on completion
        // and signals the completion channel so `stop_all_and_wait` can drain.
        drop(tokio::task::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                hitz_vmm::boot_and_run(&*hv, &config, serial_buf, stop_flag)
            })
            .await;

            // Update state based on result.
            if let Ok(mut vms) = vms.lock()
                && let Some(entry) = vms.get_mut(&vm_id)
            {
                match result {
                    Ok(Ok(run_result)) => match run_result.exit_reason {
                        ExitReason::Halt | ExitReason::Canceled => {
                            entry.state = VmState::Stopped;
                            entry.exit_reason = Some(format!("{:?}", run_result.exit_reason));
                        }
                        reason => {
                            entry.state = VmState::Failed;
                            entry.exit_reason = Some(format!("{reason:?}"));
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
                if let Some(ref buf) = entry.serial_buf {
                    buf.close();
                }
            }

            // Notify stop_all_and_wait that this VM has finished.
            let _ = completion_tx.send(vm_id);
        }));

        Ok(info)
    }

    /// Stop a running VM by setting its stop flag.
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

    /// Get a serial output reader for a VM.
    ///
    /// Returns a [`SerialReader`] that can be polled for chunks of serial
    /// output.  Only available while the VM is running (i.e. has a serial
    /// buffer attached).
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

    /// Get info about a single VM.
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

    /// List all VMs.
    pub fn list_vms(&self) -> Result<Vec<VmInfo>, DaemonError> {
        let vms = self
            .vms
            .lock()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;
        Ok(vms.iter().map(|(id, entry)| entry.to_info(id)).collect())
    }

    /// Delete a VM. Must not be running.
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
        Ok(())
    }

    /// Stop all running VMs (fire-and-forget, does not wait for them to finish).
    ///
    /// Sets the stop flag on every running VM, which signals the cancel-watchdog
    /// threads to cancel the vCPU run loops. Returns immediately without waiting
    /// for the VMs to reach a terminal state. Use [`stop_all_and_wait`] if you
    /// need to block until all VMs have stopped.
    ///
    /// [`stop_all_and_wait`]: VmManager::stop_all_and_wait
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

    /// Stop all running VMs and wait for them to finish.
    ///
    /// Sets stop flags for all running VMs (triggering their internal
    /// cancel-watchdog threads), then waits on the completion channel
    /// until all VMs report done or `timeout` expires.
    ///
    /// If the timeout elapses before all VMs stop, a warning is logged and
    /// the method returns — it does not forcibly kill any remaining VMs.
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

    fn make_manager() -> VmManager<FakeHypervisor> {
        VmManager::new(Arc::new(FakeHypervisor))
    }

    fn make_config() -> VmConfig {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        // Leak the temp file so it survives the test.
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        VmConfig {
            kernel_path: path,
            initramfs_path: None,
            disk_path: None,
            ram_mib: 128,
            cpus: 1,
            cmdline: None,
            net: None,
        }
    }

    #[test]
    fn create_vm_sets_created_state() {
        let mgr = make_manager();
        let info = mgr.create_vm("vm1".into(), make_config()).expect("create");
        assert_eq!(info.state, VmState::Created);
        assert_eq!(info.id, "vm1");
    }

    #[test]
    fn create_duplicate_vm_fails() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        let err = mgr.create_vm("vm1".into(), make_config()).unwrap_err();
        assert!(err.to_string().contains("already exists"), "got: {err}");
    }

    #[test]
    fn get_vm_not_found() {
        let mgr = make_manager();
        let err = mgr.get_vm("nope").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn list_vms_empty() {
        let mgr = make_manager();
        let list = mgr.list_vms().expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn list_vms_after_create() {
        let mgr = make_manager();
        mgr.create_vm("a".into(), make_config()).expect("create a");
        mgr.create_vm("b".into(), make_config()).expect("create b");
        let list = mgr.list_vms().expect("list");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_created_vm() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        mgr.delete_vm("vm1").expect("delete");
        let err = mgr.get_vm("vm1").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_nonexistent_vm_fails() {
        let mgr = make_manager();
        let err = mgr.delete_vm("nope").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn stop_created_vm_fails() {
        let mgr = make_manager();
        mgr.create_vm("vm1".into(), make_config()).expect("create");
        let err = mgr.stop_vm("vm1").unwrap_err();
        assert!(err.to_string().contains("Created"), "got: {err}");
    }

    #[test]
    fn serial_buf_available_after_start() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build runtime");

        rt.block_on(async {
            let mgr = make_manager();
            mgr.create_vm("vm1".into(), make_config()).expect("create");

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
}
