# 🔭 Vantage: Spec for Live Migration

## Problem Statement
Currently, a Hitz microVM is bound to the physical host on which it started. To perform hardware maintenance, upgrade the host OS, or rebalance cluster resources, an administrator must shut down the microVM, move its data, and boot it on another node. This causes unacceptable downtime for production workloads and violates SLAs.

## The "So What?"
What business problem does this solve? Enterprise customers and cloud providers demand high availability. Live migration enables zero-downtime node maintenance and dynamic load balancing. Without it, maintaining the fleet means breaking customer applications. With it, Hitz can compete with vSphere and KVM in mission-critical environments.

## Gap Analysis
Standard hypervisors (KVM, Hyper-V) support live migration out of the box. Firecracker does not explicitly support live migration across hosts without complex user-space orchestration built on top of its snapshot/restore capabilities. Since Phase 16 already introduced snapshot and restore functionality, we have the primitive building blocks (memory dumping and CPU state serialization). The gap is an orchestration layer and network transport to synchronize memory iteratively while the VM continues running (pre-copy migration).

## Acceptance Criteria
- **User Story:** As a Cluster Administrator, I want to seamlessly move a running microVM to a different physical host so that I can perform zero-downtime hardware maintenance without impacting end-user traffic.
- **Metric Definition:** Success = Total VM downtime during the migration cut-over phase must be less than 150ms for 99% of migrations. The microVM's network connections must remain active after the move.
- **Functional Requirements:**
  - The API must expose a `/api/v1/vms/{id}/migrate` endpoint.
  - Implement a pre-copy memory synchronization algorithm (iteratively copying dirty pages using KVM/WHP dirty page tracking).
  - Suspend, transfer final state (registers + last dirty pages), and resume on the target host.
  - Seamlessly re-announce the MAC address on the new host's network via Gratuitous ARP (or coordinate with the SDN).

## Out of Scope
- **Post-copy Migration:** Transferring execution to the target before all memory is copied (page-fault based fetching) is too complex for Phase 1.
- **Shared Storage:** We assume the VM's backing block devices are already available on the target node (e.g., via iSCSI, Ceph, or a distributed filesystem). Storage migration (moving the virtual disks) is out of scope.
- **Cross-Architecture Migration:** Migrating from an Intel host to an AMD host (or ARM) is explicitly prohibited.
