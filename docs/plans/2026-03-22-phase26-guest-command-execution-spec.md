# 🔭 Vantage: Spec for Guest Command Execution

## Problem Statement
Currently, to execute a command inside a running Hitz microVM, users must rely on external networking solutions like SSH or bridge networks. This introduces significant friction, as it requires opening network ports, managing SSH keys, and ensuring network configurations are correct. This approach is prone to errors, slow to set up, and poses a security risk by exposing the microVM's internal network. Users need a reliable, network-independent way to execute commands inside the guest OS.

## The "So What?"
What business problem does this solve? Frictionless management and automation are critical for CI/CD pipelines, automated testing, and general developer workflows. If a system administrator or a CI script cannot easily run diagnostic commands or launch processes inside the VM without complex networking setup, they will look for alternative tools. Providing a zero-configuration, secure, out-of-band command execution mechanism over the virtio-vsock channel eliminates this barrier. It drastically reduces setup time, improves security by eliminating the need for SSH, and makes Hitz a much more powerful and self-contained automation platform.

## Gap Analysis
While we have virtio-vsock configured for metrics (Phase 12) and file transfer (Phase 25), we currently lack a protocol for launching and controlling arbitrary processes within the guest. We lack a generic remote execution feature that operates strictly over the hypervisor socket. Standard tools like SSH require an external network stack (TCP/IP) and a running daemon, conflicting with our minimal initramfs philosophy. We need a purpose-built remote execution capability natively integrated with the guest agent.

## Acceptance Criteria
- 👤 **User Story:** As a System Administrator, I want to execute a command inside a running microVM directly from the host CLI, so that I can troubleshoot the guest OS without configuring SSH or external networks.
- 👤 **User Story:** As a CI Pipeline, I want to run a test script inside the microVM and capture its exit code, stdout, and stderr, so that I can automatically verify the build's success.
- ✅ **Metric Definition:** Success = A command execution request sent from the host is processed by the guest agent, and the exit code, stdout, and stderr are successfully returned to the host within 500ms of the command's completion, handling output streams up to 10MB without truncating.
- **Functional Requirements:**
  - Add a CLI command `hitz vm exec <vm_id> -- <command> [args...]`.
  - The daemon must forward the execution request over an established virtio-vsock channel.
  - The guest agent must securely spawn the requested process, capture its standard output, standard error, and exit status.
  - The guest agent must stream the output back to the daemon over the vsock channel.
  - The CLI must exit with the same exit code as the command executed within the guest.

## 🚫 Out of Scope
- **Interactive PTY Sessions:** Phase 26 strictly focuses on non-interactive command execution (batch processing/scripts). Full interactive shell access with PTY allocation, cursor handling, and signal forwarding (like Ctrl+C to the guest process) is out of scope.
- **Long-Running Process Management:** We are building a mechanism to execute a command and wait for it to finish. Managing background services or detaching/reattaching to processes is out of scope.
