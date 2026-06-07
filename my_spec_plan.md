# 🔭 Vantage: Spec for Interactive PTY Sessions

## Problem Statement
The CLI lacks an interactive way to run commands. The lack of interactive shell session requires users to repeatedly type the `hitz exec` command to execute a bash command. This adds a large amount of overhead for interactive diagnostics, as well as breaking some workflows.

## The "So What?"
What business problem does this solve? Friction in diagnostics and execution. Adding a PTY reduces user frustration by avoiding continuous `exec` commands to execute basic command lines.

## Gap Analysis
- **Current State:** The current CLI supports running non-interactive commands.
- **Market Standard:** Docker and other CRI tools natively support interactive shell sessions to execute terminal applications.

## Acceptance Criteria
- 👤 **User Story:** As a developer, I want to start an interactive shell to debug a micro-VM, so that I can diagnose issues.
- ✅ **Metric Definition:** Success = Executing `hitz exec --interactive` provides an interactive pseudo-terminal connected to the VM.
- **Functional Requirements:**
  - Add support for interactive mode via virtio-console.

## 🚫 Out of Scope
- **SSH Support:** We use virtio-console, not an SSH daemon.
