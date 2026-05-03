# 🔭 Vantage: Spec for Synchronized VM Shutdown

## Problem Statement
Currently, when a Hitz microVM is commanded to shut down, the shutdown signal is not reliably propagated to all background I/O threads (such as the virtio-net polling threads). This results in "zombie" threads that continue to run after the main VM process has exited, leading to resource leaks, locked network adapters, and potential instability when starting subsequent VMs.

## The "So What?"
What business problem does this solve? Reliability and resource efficiency. In a high-density microVM environment (like a serverless function platform or CI/CD pipeline), VMs are rapidly created and destroyed. If each shutdown leaves behind orphaned I/O threads, the host machine will eventually exhaust its resources (CPU, memory, handles) and crash. Synchronizing the shutdown signal ensures clean, deterministic termination of all VM components, preventing resource exhaustion and reducing operational support costs.

## Gap Analysis
- **Current State:** The VMM uses a `stop_flag` to signal the main run loop to exit, but network I/O threads use a disconnected, un-synchronized boolean flag (as noted by a `TODO: sync with parent stop_flag` in the Phase 7 plan). The parent shutdown signal does not cascade to the network threads.
- **Market Standard:** Production-grade hypervisors (like Firecracker and QEMU) guarantee that a VM termination event completely destroys all associated worker threads and releases all file descriptors/handles before returning success to the control plane.

## Acceptance Criteria
- **User Story:** As a Platform Operator, I want all background I/O threads to terminate immediately when a VM is shut down, so that I can run thousands of ephemeral VMs without leaking host resources.
- **Metric Definition:** Success = A test that creates and destroys 100 VMs sequentially leaves exactly 0 orphaned `hitz_net` or `hitz_devices` background threads running on the host, and all memory/handles are fully reclaimed within 1 second of the stop command.
- **Functional Requirements:**
  - The VM control plane must use a unified, synchronized cancellation token or signaling mechanism.
  - When the main VM is stopped (either gracefully or forcefully), the signal must securely propagate to all active I/O threads (e.g., networking, block devices).
  - The daemon must wait for all I/O threads to join/exit before reporting the VM as fully terminated.

## 🚫 Out of Scope
- **Guest-Initiated ACPI Shutdown:** This phase focuses on host-initiated termination (e.g., the user clicking "Stop" or sending a SIGTERM). Guest-initiated clean shutdowns (e.g., typing `poweroff` inside the guest) are handled separately.
