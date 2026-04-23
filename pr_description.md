# 🔭 Vantage: Spec for TCP Echo Test Initramfs

👤 **User Story:** As a VMM Developer, I want a test initramfs containing a TCP echo server, so that I can automatically verify bi-directional traffic flow across forwarded ports in integration tests.

✅ **Acceptance Criteria:**
- Build a minimal Linux initramfs with a TCP listener on port 9999 that echoes data.
- The `phase10_port_forward_tcp` test must connect to `127.0.0.1:19999` and successfully receive an echoed payload back within a 5-second timeout.
- Provide a reproducible build script for the initramfs.

🚫 **Out of Scope:**
- UDP port forwarding verification.
- Full Linux distributions (must be minimal).
