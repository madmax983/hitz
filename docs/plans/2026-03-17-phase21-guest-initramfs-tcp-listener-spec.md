# 🔭 Vantage: Spec for End-to-End Network Data Plane Verification

## Problem Statement
Currently, our integration testing capabilities for network port forwarding can only verify that the virtual machine boots and that the platform accepts the port mapping configuration. They cannot fully validate the end-to-end data plane because the default guest environment used in tests lacks a running service to receive and respond to incoming network traffic.

## The "So What?"
What business problem does this solve? Testing infrastructure is the bedrock of reliable product delivery. Without end-to-end networking tests, regressions in our virtual networking interface or host-side port forwarding implementation could slip into production unnoticed, leading to broken network connectivity for users. By implementing a guest-side network listener, we can guarantee that port forwarding and network traffic are flowing correctly between the host and the virtual machine, saving support costs and protecting user trust.

## Gap Analysis
Our integration test suite currently leaves full end-to-end connectivity assertions incomplete. While the host-side hypervisor management and networking logic correctly configure the port bindings, the testing sequence stops short of transmitting actual data because there is no service inside the guest listening on the mapped ports. We are missing a minimal, purpose-built testing environment containing a lightweight TCP echo service.

## Acceptance Criteria
- 👤 **User Story:** As a Core Engineer, I want the test guest environment to run a TCP echo service on boot, so that I can write end-to-end integration tests that send packets from the host and verify the responses.
- ✅ **Metric Definition:** Success = A network integration test successfully establishes a TCP connection to the host-mapped port, sends a string of bytes, and receives the exact same string of bytes back from the guest without timing out, achieving 100% reliability in continuous integration.
- **Functional Requirements:**
  - Provide a reproducible mechanism to generate a minimal testing guest image.
  - The guest image must start a lightweight TCP service immediately after network interface initialization.
  - The test suite must be updated to boot this specific image and assert full bi-directional TCP connectivity.

## 🚫 Out of Scope
- **Production Images:** This is strictly for integration testing. We are not building a production-ready operating system image.
- **Complex Application Protocols:** We only need to verify basic TCP byte-streaming (echo), not HTTP, TLS, or other higher-level protocols.
- **UDP Testing:** The current focus is strictly on TCP. UDP will be handled in a separate phase if necessary.
