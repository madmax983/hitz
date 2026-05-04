# Spec: TCP Echo Test Initramfs

👤 **User Story:** As a VMM Developer, I want a test initramfs containing a TCP echo server, so that I can automatically verify bi-directional traffic flow across forwarded ports in integration tests.

✅ **Acceptance Criteria:**
- **Metric Definition:** Success = A network integration test successfully connects to a forwarded port on the host, sends a payload, and receives the exact same payload back from the guest within a 5-second timeout, achieving 100% reliability.
- **So What? (Business Problem):** Without end-to-end verification of port forwarding in our integration tests, we risk regressions in the network and daemon routing components going unnoticed. Ensuring that port forwarding genuinely works at the TCP level is critical for developer confidence and guaranteeing the reliability of our network emulation layer for end users.
- **Gap Analysis:** We have an environment variable gating the port forwarding test, but no official minimal initramfs image bundled or generated in our test suite that starts a TCP echo server on boot. Standard libraries do not magically provide this; we must explicitly provide a test image.

🚫 **Out of Scope:**
- Full Linux distributions; the image must be minimal.
- UDP port forwarding verification.
- Performance/throughput benchmarking of the network interface.
