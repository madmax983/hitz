# 🔭 Vantage: Spec for TCP Echo Test Initramfs

## Problem Statement
Currently, our `hitz-whp` test suite marks the "Full TCP connect + echo assertion" as a TODO. While the host-side VMM and networking logic correctly configure the port bindings, the test stops short of sending actual bytes because there is nothing inside the guest listening on the mapped port.

## The "So What?"
What business problem does this solve? Testing infrastructure is the bedrock of reliable product delivery. Without end-to-end networking tests, regressions in our virtio-net or host-side port forwarding implementation could slip into production unnoticed, leading to broken network connectivity for users. By implementing a guest-side TCP listener, we can guarantee that port forwarding and network traffic are flowing correctly between the host and the microVM, saving support costs and protecting user trust.

## Gap Analysis
- **Current State:** The integration test lacks a guest component that actively listens and responds to TCP traffic.
- **Market Standard:** End-to-end tests for network virtualization require a verifiable server in the guest. We are missing a minimal, purpose-built initramfs containing a lightweight TCP server.

## Acceptance Criteria
- 👤 **User Story:** As a Core Engineer, I want the test guest initramfs to run a TCP echo server on boot, so that I can write end-to-end integration tests that send packets from the host and verify the responses.
- ✅ **Metric Definition:** Success = A new integration test successfully establishes a TCP connection to the host-mapped port, sends a string of bytes, and receives the exact same string of bytes back from the guest without timing out.
- **Functional Requirements:**
  - Build or provide a reproducible script to generate a minimal Linux initramfs.
  - The initramfs must start a TCP service (like `nc -l -p 9999 -e cat` or a custom lightweight Rust binary) immediately after network interface initialization.
  - The test suite must be updated to boot this specific initramfs and assert full TCP connectivity.

## 🚫 Out of Scope
- **Production Initramfs:** This is strictly for integration testing. We are not building a production-ready OS image.
- **Complex Application Protocols:** We only need to verify basic TCP byte-streaming (echo), not HTTP, TLS, or other higher-level protocols.
- **UDP Testing:** For Phase 21, the focus is strictly on TCP. UDP will be handled in a separate phase if necessary.
