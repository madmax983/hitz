# 🔭 Vantage: Spec for Network Thread Stop Flag Synchronization

## Problem Statement
Currently in Phase 7, the network thread initialization block contains a critical TODO. When the microVM boots, the virtual network I/O thread is provided its own detached, unsynchronized shutdown signal rather than safely sharing or synchronizing with the parent VM's stop flag. This means if the VMM attempts a graceful shutdown or experiences a panic, the network thread may continue running indefinitely, leading to resource leaks, port collisions, and zombie processes in the daemon.

## The "So What?"
What business problem does this solve? Stability and Resource Management. For enterprise environments running thousands of microVMs on a single node, orphaned background threads are a critical failure mode. If network I/O threads do not cleanly exit when the main VM shuts down, the daemon will eventually hit file descriptor limits, consume excess CPU, and require hard restarts. Fixing this ensures predictable, clean teardowns, which is mandatory for any production-grade serverless platform or Kubernetes integration.

## Gap Analysis
- **Current State:** The network I/O thread receives a newly allocated shutdown signal instead of correctly inheriting or wrapping the parent VM's stop signal.
- **Market Standard:** Enterprise runtimes guarantee that all auxiliary device threads (net, block, serial) are synchronized to a single kill/stop signal, ensuring 100% clean exit on shutdown.

## Acceptance Criteria
- 👤 **User Story:** As a Platform Operator, I want the microVM's background network threads to shut down synchronously when the VM exits, so that host resources are cleanly released and zombie threads do not accumulate.
- ✅ **Metric Definition:** Success = Executing a graceful shutdown results in all associated network I/O threads terminating within 500ms, with zero orphaned threads remaining in the daemon process.
- **Functional Requirements:**
  - The network thread initialization logic must correctly inherit or observe the parent VM's stop signal.
  - The TODO comment must be removed.
  - Integration tests must verify that network threads exit cleanly after a VM halt.

## 🚫 Out of Scope
- **Graceful connection draining:** Terminating the thread immediately upon the stop signal is acceptable; we do not need to wait for active TCP connections to finish draining.
- **Other device threads:** This spec is strictly scoped to the network I/O thread stop signal, not block or serial devices.
