# 🔭 Vantage: Spec for Interactive PTY Sessions

## 👤 User Story
As a Developer, I want to open an interactive bash shell inside a running microVM directly from my host terminal, so that I can explore the filesystem and debug processes in real-time.
As a System Administrator, I want my terminal window resizes to accurately propagate to the guest session, so that tools like `top` render correctly.

## 💡 The "So What?" (Business Problem)
Debugging complex runtime issues often requires human-in-the-loop interaction rather than pre-scripted automation. If developers cannot "SSH into" a microVM securely out-of-the-box, they are forced to configure external networking and SSH daemons, increasing friction and security risks. By providing native, interactive PTY sessions directly over virtio-vsock, we dramatically improve the developer experience and reduce the Mean Time To Resolution (MTTR) for debugging, cementing Hitz as a first-class platform for both development and production.

## ✅ Acceptance Criteria
- **Metric Definition:** Success = A user can request an interactive session. The session allocates a PTY in the guest, forwards terminal resizing events, relays raw keystrokes (including signals like Ctrl+C and Ctrl+D), and terminates cleanly when the shell exits, all within 200ms latency.
- **Functional Requirements:**
  - Introduce an interactive flag to the guest command execution CLI.
  - The daemon and guest agent must negotiate PTY allocation prior to spawning the interactive process.
  - The client CLI must switch the host terminal into raw mode and stream unbuffered byte streams bidirectionally.
  - The protocol must support sending out-of-band control messages, such as terminal window resize events (`SIGWINCH`), over the established vsock channel.

## 🔍 Gap Analysis
Our current command execution (Phase 26) supports only standard batch execution—capturing `stdout` and `stderr` linearly and returning an exit code. Standard hypervisors and container runtimes offer full pseudo-terminal (PTY) allocation, resizing events, and raw signal forwarding (e.g., Ctrl+C to terminate a guest process). We lack the control plane messages and host-to-guest terminal emulation required to support interactive applications.

## 🚫 Out of Scope
- **Graphical (X11/Wayland) Forwarding:** The scope is strictly limited to text-based terminal emulation. We are not supporting graphical application forwarding.
- **Persistent Sessions (Tmux/Screen equivalent):** Hitz will not attempt to implement session persistence across network disconnects. If the CLI client disconnects, the PTY session and its children are terminated.
