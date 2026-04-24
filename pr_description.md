* 👤 **User Story:** As a VMM Developer, I want a test initramfs containing a TCP echo server, so that I can automatically verify bi-directional traffic flow across forwarded ports in integration tests.
* ✅ **Acceptance Criteria:**
  - Must build a minimal Linux initramfs.
  - Must configure loopback and virtio-net interfaces.
  - Must start a TCP listener on port 9999 that echoes received bytes.
  - Must provide a script or documentation on how to regenerate this initramfs.
  - Must connect to 127.0.0.1:19999 and assert the echo response.
* 🚫 **Out of Scope:** Full Linux distributions, UDP port forwarding verification, Performance/throughput benchmarking.
