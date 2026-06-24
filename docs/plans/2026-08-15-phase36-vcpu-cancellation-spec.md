# 🔭 Vantage: Spec for VCPU Cancellation API

## Problem Statement
Currently, the virtual processor implementation has a gap in its interface for cancellation handles. Because of this, developers cannot gracefully interrupt a running VCPU to perform operations like graceful shutdown, interrupt injection, or pausing the VM.

## The "So What?"
What business problem does this solve? Graceful VM management. Without the ability to safely cancel a running VCPU, shutting down a VM requires force-killing the entire process, which risks data corruption. Implementing the cancellation API allows Hitz to shut down cleanly and support advanced lifecycle states, improving reliability for end users.

## Gap Analysis
- **Current State:** The virtual processor implementation contains missing implementations for cancellation handles, preventing asynchronous interruption of the hypervisor execution loop.
- **Market Standard:** Modern hypervisors (QEMU, Firecracker) provide mechanisms to kick or cancel VCPUs via signals or eventfds to handle asynchronous events and graceful termination.

## Acceptance Criteria
- 👤 **User Story:** As a VMM Operator, I want to send a cancellation signal to a running VCPU, so that I can gracefully stop the micro-VM without data loss.
- ✅ **Metric Definition:** Success = A running VM can be sent a cancellation request, causing the VCPU loop to gracefully exit with a canceled state within 10ms, allowing a clean shutdown.
- **Functional Requirements:**
  - Implement the necessary hypervisor platform-specific backend calls to support cancellation handles.
  - Expose a mechanism to safely trigger cancellation from another thread.
  - Ensure the VCPU loop correctly handles the cancellation exit and cleans up resources.

## 🚫 Out of Scope
- **Live Migration:** Pausing for live migration is out of scope; this is purely for graceful shutdown.
