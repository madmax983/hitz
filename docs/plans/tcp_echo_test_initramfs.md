## Problem Statement
The `phase10_port_forward_tcp` integration test currently only verifies that a VM boots and exits cleanly with port forwarding configured (`ports: [19999:9999]`). It lacks a mechanism to verify that network traffic can actually flow over the forwarded port to the guest. A full TCP connect and echo assertion is currently a TODO because we do not have a test initramfs containing a TCP listener on port 9999.

## The "So What?"
Without end-to-end verification of port forwarding in our integration tests, we risk regressions in the `hitz-net` and daemon routing components going unnoticed. Ensuring that port forwarding genuinely works at the TCP level is critical for developer confidence and guaranteeing the reliability of our network emulation layer for end users.

## Gap Analysis
We have an environment variable (`HITZ_TEST_INITRAMFS`) gating the port forwarding test, but no official minimal initramfs image bundled or generated in our test suite that starts a TCP echo server (e.g., using `nc -lp 9999` or a custom rust binary) on boot. Standard libraries do not magically provide this; we must explicitly provide a test image.

## Acceptance Criteria

👤 **User Story:** As a VMM Developer, I want a test initramfs containing a TCP echo server, so that I can automatically verify bi-directional traffic flow across forwarded ports in integration tests.

✅ **Metric Definition:** Success = The `phase10_port_forward_tcp` test connects to `127.0.0.1:19999` on the host, sends a payload, and successfully receives the exact same payload back from the guest VM within a 5-second timeout, achieving 100% reliability over 100 consecutive runs.

- **Functional Requirements:**
  - Build a minimal Linux initramfs.
  - The init script (`/init`) must configure the loopback and virtio-net interfaces.
  - Start a TCP listener on port 9999 that echoes received bytes back to the sender.
  - Provide a script or documentation on how to regenerate this initramfs.
  - Update `crates/hitz-whp/src/tests.rs` to connect to `127.0.0.1:19999` and assert the echo response.

## 🚫 Out of Scope
- Full Linux distributions; the image must be minimal (e.g., built with Buildroot or pure static binaries).
- UDP port forwarding verification.
- Performance/throughput benchmarking of the network interface.
