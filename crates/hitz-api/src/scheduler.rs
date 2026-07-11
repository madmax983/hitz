//! Fleet scheduler logic.
//!
//! # Abstract
//! This module provides logic for scheduling micro-VMs across a cluster of physical host nodes.

use crate::VmConfig;
use serde::{Deserialize, Serialize};

/// Represents a physical host node in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostNode {
    /// The unique identifier of the host.
    pub id: String,
    /// Total available CPU cores on this host.
    pub total_cpus: u32,
    /// Total available RAM in MiB on this host.
    pub total_ram_mib: u32,
    /// The number of CPU cores already allocated.
    pub allocated_cpus: u32,
    /// The amount of RAM already allocated in MiB.
    pub allocated_ram_mib: u32,
}

impl HostNode {
    /// Checks if the given VM can fit onto this host based on available resources.
    #[must_use]
    pub const fn can_fit(&self, vm: &VmConfig) -> bool {
        self.total_cpus >= self.allocated_cpus + vm.cpus
            && self.total_ram_mib >= self.allocated_ram_mib + vm.ram_mib
    }

    /// Allocates resources on this host for the given VM.
    pub const fn allocate(&mut self, vm: &VmConfig) {
        self.allocated_cpus += vm.cpus;
        self.allocated_ram_mib += vm.ram_mib;
    }
}

/// Represents the placement of a VM on a specific host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allocation {
    /// The unique identifier of the VM.
    pub vm_id: String,
    /// The unique identifier of the host it was placed on.
    pub host_id: String,
}

/// Trait defining the behavior of a fleet scheduler.
pub trait Scheduler {
    /// Schedules a set of VMs onto a set of host nodes.
    ///
    /// # Errors
    /// Returns an error string if there is insufficient capacity to place all VMs.
    fn schedule(&self, hosts: &mut [HostNode], vms: &[(String, VmConfig)]) -> Result<Vec<Allocation>, String>;
}

/// A scheduler that uses the First-Fit Decreasing algorithm.
pub struct FirstFitScheduler;

impl Scheduler for FirstFitScheduler {
    fn schedule(&self, hosts: &mut [HostNode], vms: &[(String, VmConfig)]) -> Result<Vec<Allocation>, String> {
        let mut allocations = Vec::new();

        let mut sorted_vms = vms.to_vec();
        // Sort VMs by size descending (First-Fit Decreasing)
        sorted_vms.sort_by(|a, b| (b.1.cpus + b.1.ram_mib).cmp(&(a.1.cpus + a.1.ram_mib)));

        for (vm_id, vm) in sorted_vms {
            let mut placed = false;
            for host in hosts.iter_mut() {
                if host.can_fit(&vm) {
                    host.allocate(&vm);
                    allocations.push(Allocation {
                        vm_id: vm_id.clone(),
                        host_id: host.id.clone(),
                    });
                    placed = true;
                    break;
                }
            }
            if !placed {
                return Err(format!("Insufficient capacity for VM {vm_id}"));
            }
        }

        Ok(allocations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuestAgentMode;
    use std::path::PathBuf;

    fn dummy_vm(cpus: u32, ram_mib: u32) -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from(""),
            initramfs_path: None,
            disk_path: None,
            ram_mib,
            cpus,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_first_fit_success() {
        let mut hosts = vec![
            HostNode { id: "h1".into(), total_cpus: 4, total_ram_mib: 4096, allocated_cpus: 0, allocated_ram_mib: 0 },
            HostNode { id: "h2".into(), total_cpus: 2, total_ram_mib: 2048, allocated_cpus: 0, allocated_ram_mib: 0 }
        ];

        let vms = vec![
            ("v1".into(), dummy_vm(2, 2048)),
            ("v2".into(), dummy_vm(2, 2048)),
            ("v3".into(), dummy_vm(1, 1024))
        ];

        let scheduler = FirstFitScheduler;
        let allocations = scheduler.schedule(&mut hosts, &vms).unwrap();

        assert_eq!(allocations.len(), 3);
    }

    #[test]
    fn test_first_fit_insufficient() {
        let mut hosts = vec![
            HostNode { id: "h1".into(), total_cpus: 2, total_ram_mib: 2048, allocated_cpus: 0, allocated_ram_mib: 0 }
        ];

        let vms = vec![
            ("v1".into(), dummy_vm(4, 4096)),
        ];

        let scheduler = FirstFitScheduler;
        assert!(scheduler.schedule(&mut hosts, &vms).is_err());
    }
}
