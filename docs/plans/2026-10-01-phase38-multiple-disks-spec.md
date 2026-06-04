# 🔭 Vantage: Spec for Multiple Block Devices

## Problem Statement
Currently, a Hitz microVM supports attaching only a single block device as the primary disk. This restricts users from deploying workloads that require separate volumes for the operating system and application data, or from dynamically attaching secondary storage volumes for backups or high-performance scratch space.

## The "So What?"
What business problem does this solve? Flexibility and data persistence. Enterprise workloads commonly separate the ephemeral root filesystem from persistent, stateful data volumes (like a database data directory). By supporting multiple block devices, Hitz enables users to deploy stateful applications using industry-standard architectures, unlocking a wider range of use cases and improving data safety by isolating user data from the OS.

## Gap Analysis
- **Current State:** The VM configuration allows only one disk path, and the device initialization logic hardcodes a single virtio-block device.
- **Market Standard:** Competing platforms (e.g., AWS EC2, Firecracker) allow attaching multiple block devices to a single instance, often differentiating between root volumes and data volumes.

## Acceptance Criteria
- 👤 **User Story:** As a Database Operator, I want to attach a secondary block device to my microVM, so that I can store my database files on a separate persistent volume from the root OS.
- ✅ **Metric Definition:** Success = A user can define two distinct block devices in the VM configuration, the VM boots successfully, and the guest OS recognizes both devices, allowing read/write operations to both.
- **Functional Requirements:**
  - Update the configuration interface to accept a list of block devices instead of a single disk.
  - The hypervisor must instantiate and map a virtual block device for each configured entry.
  - Ensure the guest kernel correctly enumerates multiple virtio-blk devices on the MMIO bus.

## 🚫 Out of Scope
- **Dynamic Hotplugging:** Adding or removing block devices while the VM is running is out of scope for this phase. Device attachment occurs only at boot time.
- **Network-Attached Storage:** This phase focuses on local block devices (file-backed). iSCSI or NFS integration is excluded.
