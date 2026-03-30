# Phase 16: Snapshot & Restore — Spec

## Problem Statement
Currently, a Hitz microVM must boot from scratch every time. This limits the ability to rapidly scale out identical workloads, pause compute to save resources, or migrate stateful applications without losing in-memory data.

## The "So What?"
Users want faster boot times for complex workloads and the ability to suspend execution to save costs. Without snapshot/restore capabilities, Hitz cannot support "serverless" scale-to-zero architectures or live migration scenarios, limiting its utility in dynamic orchestration environments.

## Goal
Enable capturing the complete execution state (vCPU registers, memory, and device state) of a running microVM to disk, and reliably resuming execution from that state later.

## Gap Analysis & Assumptions
**Market Comparison:**
- Firecracker supports microVM snapshots with <10ms pause times and local state restoration.
- Cloud Hypervisor supports both local snapshots and live migration.
- **Our Gap:** Hitz currently has no capability to pause or save state. We will implement local file-based snapshots first (similar to Firecracker's model) before attempting complex network-based live migration.

**Assumptions:**
- We assume the host filesystem is fast enough (e.g., NVMe SSD) to write the guest memory snapshot without causing unacceptable downtime, or we will pause the VM during the entire write operation.
- We assume the user is responsible for managing the generated `.hitz` snapshot artifacts.

## Scope

**In Scope:**
- Pausing a running VM execution.
- Serializing vCPU state, Guest Memory, and virtio device state to a snapshot file.
- Creating a new VM initialized from a previously saved snapshot file.
- Resuming execution from the restored state.

**Out of Scope:**
- Live migration over the network (Phase 17).
- Snapshotting network connections that have timed out on the external host.
- Incremental or differential memory snapshots.

## Acceptance Criteria
- **User Story:** As a Site Reliability Engineer, I want to snapshot a running microVM and restore it later, so that I can scale workloads quickly without booting from scratch.
- **Metric:** Snapshot creation time must be < 500ms for a 128MB microVM. Restore time must be < 200ms.
- **Functionality:**
  - The API and CLI must provide `pause`, `snapshot`, and `restore` commands.
  - The snapshot artifact must be a single file (or defined directory structure) containing all necessary state.
  - A restored VM must resume execution at the exact instruction it was paused at, with no guest kernel panic.

## Proposed API Additions

### `VmAction` Updates
```json
{
  "action": "Pause"
}
```
```json
{
  "action": "Snapshot",
  "destination_path": "/path/to/snapshot.hitz"
}
```

### CLI Updates
```bash
hitz vm pause <vm_id>
hitz vm snapshot <vm_id> --output ./snapshot.hitz
hitz vm restore --snapshot ./snapshot.hitz
```
