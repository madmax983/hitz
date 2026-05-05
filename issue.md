# 🔭 Vantage: Spec for Synchronized VM Shutdown

👤 **User Story:** "As a Platform Operator, I want all background I/O threads to terminate immediately when a VM is shut down, so that I can run thousands of ephemeral VMs without leaking host resources."

✅ **Acceptance Criteria:**
- **Metric Definition:** Success = A test that creates and destroys 100 VMs sequentially leaves exactly 0 orphaned `hitz_net` or `hitz_devices` background threads running on the host, and all memory/handles are fully reclaimed within 1 second of the stop command.
- **So What? (Business Problem):** Reliability and resource efficiency. In a high-density microVM environment (like a serverless function platform or CI/CD pipeline), VMs are rapidly created and destroyed. If each shutdown leaves behind orphaned I/O threads, the host machine will eventually exhaust its resources (CPU, memory, handles) and crash. Synchronizing the shutdown signal ensures clean, deterministic termination of all VM components, preventing resource exhaustion and reducing operational support costs.
- **Gap Analysis:** The VMM uses a `stop_flag` to signal the main run loop to exit, but network I/O threads use a disconnected, un-synchronized boolean flag (as noted by a `TODO: sync with parent stop_flag` in the Phase 7 plan). The parent shutdown signal does not cascade to the network threads. Market standard hypervisors guarantee that a VM termination event completely destroys all associated worker threads.
- **Functional Requirements:**
  - The VM control plane must use a unified, synchronized cancellation token or signaling mechanism.
  - When the main VM is stopped (either gracefully or forcefully), the signal must securely propagate to all active I/O threads (e.g., networking, block devices).
  - The daemon must wait for all I/O threads to join/exit before reporting the VM as fully terminated.

🚫 **Out of Scope:**
- **Guest-Initiated ACPI Shutdown:** This phase focuses on host-initiated termination (e.g., the user clicking "Stop" or sending a SIGTERM). Guest-initiated clean shutdowns (e.g., typing `poweroff` inside the guest) are handled separately.
