# 🔭 Vantage: Spec for Dynamic Volume Hotplugging

## Problem Statement
Currently, Hitz requires all block devices (rootfs, cloud-init ISO) to be specified at VM boot time in `VmConfig`. If a user needs to attach a new data volume (e.g., a persistent database drive) to a running microVM, they must shut down the VM, update the configuration, and restart it. This causes unacceptable downtime for stateful workloads.

## The "So What?"
Users want to run orchestrators like Kubernetes (K8s) or Nomad on top of Hitz. K8s Container Storage Interface (CSI) drivers rely heavily on the ability to dynamically attach and detach persistent volumes to running nodes. Without block device hotplugging, Hitz cannot serve as a reliable foundation for stateful cloud-native workloads, limiting its market adoption strictly to stateless functions.

## Gap Analysis
Competitors like QEMU/KVM and Firecracker support virtio-blk hotplugging via PCI/MMIO device addition. Without this capability, Hitz fails the "cattle, not pets" paradigm for stateful containers where data outlives the compute instance.

## Goal
Enable the dynamic attachment and detachment of virtio-block devices to a running microVM without requiring a reboot or interrupting guest execution.

## Scope

**In Scope:**
- Expanding the API to support `AttachVolume` and `DetachVolume` actions on a running VM.
- Dynamically mapping new host files/block devices into the guest's MMIO address space as virtio-blk devices.
- Triggering the appropriate guest OS ACPI/WHP interrupts so the Linux kernel detects the new block device (e.g., `/dev/vdb` appearing at runtime).

**Out of Scope:**
- Network interface hotplugging (virtio-net).
- Resizing existing attached volumes while running.
- Complex storage backend integrations (Ceph, iSCSI) directly within Hitz; we only handle local host files or raw block devices.

## Acceptance Criteria
- **User Story:** As a Storage Orchestrator (e.g., CSI Driver), I want to attach a new block device to a running VM so that the guest workload can mount and write to persistent storage without downtime.
- **Metric:** The `Attach` API call must return HTTP 200 OK in < 50ms, and the Linux guest kernel must recognize the new block device within 500ms.
- **Functionality:**
  - The CLI and API must support an `attach-volume` and `detach-volume` action on a running VM.
  - The guest kernel logs (`dmesg`) must show the new virtio block device initialization upon hotplug.
  - The detached volume must be flushed to disk and safely unmapped without panicking the guest kernel.

## Proposed Additions

### CLI Updates
```bash
# Attach a new volume to a running VM
hitz vm attach-volume my-db-vm --disk ./data_vol.img --device-id vdb

# Detach the volume safely
hitz vm detach-volume my-db-vm --device-id vdb
```

### API Updates
`PATCH /api/v1/vms/{id}/volumes`
```json
{
  "action": "Attach",
  "volume": {
    "device_id": "vdb",
    "path": "C:\\path\\to\\data_vol.img",
    "read_only": false
  }
}
```
