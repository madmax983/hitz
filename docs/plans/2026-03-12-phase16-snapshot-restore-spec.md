# 🔭 Vantage: Spec for Snapshot & Restore

## Problem Statement
Currently, a Hitz microVM must boot from scratch every time. This limits the ability to rapidly scale out identical workloads, pause compute to save resources, or migrate stateful applications without losing in-memory data.

## The "So What?"
What business problem does this solve? Users want faster boot times for complex workloads and the ability to suspend execution to save costs. Without snapshot/restore capabilities, Hitz cannot support "serverless" scale-to-zero architectures or live migration scenarios, limiting its utility in dynamic orchestration environments.

## Gap Analysis
- **Market Comparison:** Firecracker supports microVM snapshots with <10ms pause times and local state restoration. Cloud Hypervisor supports both local snapshots and live migration.
- **Our Gap:** Hitz currently has no capability to pause or save state. We will implement local file-based snapshots first (similar to Firecracker's model) before attempting complex network-based live migration.

## Acceptance Criteria
- 👤 **User Story:** As a Site Reliability Engineer, I want to snapshot a running microVM and restore it later, so that I can scale workloads quickly without booting from scratch.
- ✅ **Metric Definition:** Success = Snapshot creation time must be < 500ms for a 128MB microVM. Restore time must be < 200ms.
- **Functional Requirements:**
  - Pausing a running VM execution.
  - Serializing vCPU state, Guest Memory, and virtio device state to a snapshot file.
  - Creating a new VM initialized from a previously saved snapshot file.
  - Resuming execution from the restored state.

## 🚫 Out of Scope
- Live migration over the network (Phase 17).
- Snapshotting network connections that have timed out on the external host.
- Incremental or differential memory snapshots.
