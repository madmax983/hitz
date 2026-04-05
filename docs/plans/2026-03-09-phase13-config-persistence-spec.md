# 🔭 Vantage: Spec for Config Persistence

## Problem Statement
Currently, all microVM configurations and states are stored only in the memory of the `hitz` daemon. If the daemon crashes, restarts, or the host machine reboots, all information about previously created microVMs is permanently lost. Users are forced to manually recreate their entire fleet of VMs from scratch after every daemon restart.

## The "So What?"
What business problem does this solve? In production environments, infrastructure must be resilient. If the management daemon goes down, the orchestrator (e.g., Kubernetes) or the operator expects the infrastructure layer to remember what was running and recover its state gracefully. Without configuration persistence, Hitz cannot be used as a reliable foundation for long-running workloads. Persistence transforms Hitz from an ephemeral development tool into a robust, production-grade hypervisor.

## Gap Analysis
Standard hypervisors (KVM/libvirt, Hyper-V) and container runtimes (Docker, containerd) persist the state and configuration of their managed workloads to disk. When the service restarts, they load this state, identify which workloads were running, and provide accurate status information. Hitz currently lacks this fundamental reliability feature, creating a critical gap in our enterprise offering.

## Acceptance Criteria
- 👤 **User Story:** As an Infrastructure Operator, I want my VM configurations to be saved to disk, so that my VM fleet remains visible and manageable after the Hitz daemon restarts.
- ✅ **Metric Definition:** Success = After creating a VM and restarting the `hitz` daemon, `hitz vm list` correctly displays the previously created VM with its original configuration and its status accurately reflected.
- **Functional Requirements:**
  - The system must write VM configurations to persistent storage upon creation.
  - The system must track the lifecycle state of each VM persistently.
  - On daemon startup, the system must read the persistent storage and repopulate the list of known VMs.
  - If a VM was running before the daemon restarted, its state must be adjusted to indicate it is currently stopped, rather than falsely claiming it is still running.
  - Deleting a VM must remove its persistent configuration.

## 🚫 Out of Scope
- **Auto-restarting VMs:** Automatically booting VMs that were running before the restart is out of scope for Phase 13. We are only ensuring their configuration is remembered and their state is accurate.
- **Persisting VM Logs or Metrics:** We are only persisting the declarative configuration and basic lifecycle state, not runtime telemetry or stdout/stderr logs.
