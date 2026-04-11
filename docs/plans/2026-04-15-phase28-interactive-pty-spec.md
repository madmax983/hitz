# 🔭 Vantage: Spec for Interactive PTY Sessions

## Problem Statement
While Phase 26 introduced the ability to execute non-interactive commands within a running Hitz microVM, developers and system administrators still lack an interactive shell experience. Users need to dynamically explore the guest filesystem, troubleshoot issues interactively, and run command-line tools that require terminal control sequences (such as `htop` or `vim`), all without relying on SSH or exposing the microVM to external networks.

## The "So What?"
What business problem does this solve? Debugging complex runtime issues often requires human-in-the-loop interaction rather than pre-scripted automation. If developers cannot "SSH into" a microVM securely out-of-the-box, they are forced to configure external networking and SSH daemons, increasing friction and security risks. By providing native, interactive PTY sessions directly over virtio-vsock, we dramatically improve the developer experience and reduce the Mean Time To Resolution (MTTR) for debugging, cementing Hitz as a first-class platform for both development and production.

## Gap Analysis
Our current command execution (Phase 26) supports only standard batch execution—capturing `stdout` and `stderr` linearly and returning an exit code. Standard hypervisors and container runtimes (like `docker exec -it`) offer full pseudo-terminal (PTY) allocation, resizing events, and raw signal forwarding (e.g., Ctrl+C to terminate a guest process). We lack the control plane messages and host-to-guest terminal emulation required to support interactive applications.

## Acceptance Criteria
- 👤 **User Story:** As a Developer, I want to open an interactive bash shell inside a running microVM directly from my host terminal, so that I can explore the filesystem and debug processes in real-time.
- 👤 **User Story:** As a System Administrator, I want my terminal window resizes to accurately propagate to the guest session, so that tools like `top` render correctly.
- ✅ **Metric Definition:** Success = A user can execute `hitz vm exec -it <vm_id> /bin/bash`. The session allocates a PTY in the guest, forwards terminal resizing events, relays raw keystrokes (including signals like Ctrl+C and Ctrl+D), and terminates cleanly when the shell exits, all within 200ms latency.
- **Functional Requirements:**
  - Introduce an interactive flag (`-i` or `-t`) to the `hitz vm exec` command.
  - The daemon and guest agent must negotiate PTY allocation prior to spawning the interactive process.
  - The client CLI must switch the host terminal into raw mode and stream unbuffered byte streams bidirectionally.
  - The protocol must support sending out-of-band control messages, such as terminal window resize events (`SIGWINCH`), over the established vsock channel.

## 🚫 Out of Scope
- **Graphical (X11/Wayland) Forwarding:** The scope is strictly limited to text-based terminal emulation. We are not supporting graphical application forwarding.
- **Persistent Sessions (Tmux/Screen equivalent):** Hitz will not attempt to implement session persistence across network disconnects. If the CLI client disconnects, the PTY session and its children are terminated.
