//! WHP shared partition state.

use std::sync::Arc;
use windows::Win32::System::Hypervisor::{WHV_PARTITION_HANDLE, WHvDeletePartition};

/// Shared state for a WHP partition, accessible by all vCPUs.
pub struct PartitionInner {
    /// The WHP partition handle.
    pub(crate) handle: WHV_PARTITION_HANDLE,
}

// SAFETY: WHV_PARTITION_HANDLE is a process-wide handle that can be used
// from any thread. WHP APIs are documented as thread-safe per-partition.
unsafe impl Send for PartitionInner {}
unsafe impl Sync for PartitionInner {}

impl Drop for PartitionInner {
    fn drop(&mut self) {
        // SAFETY: We own the partition handle and are tearing it down.
        let _ = unsafe { WHvDeletePartition(self.handle) };
    }
}
