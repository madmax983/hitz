# 🔭 Vantage: Spec for Guest File Transfer

## Problem Statement
Currently, getting files in and out of a running Hitz microVM requires setting up a network bridge, SSH, or using an external HTTP server. These methods add significant friction, depend on external networking configurations being perfectly correct, and are insecure by default. Users need a reliable, network-independent way to move artifacts, logs, and configuration files between the host and the microVM.

## The "So What?"
What business problem does this solve? Frictionless file management is critical for CI/CD pipelines and local developer workflows. If a developer cannot easily extract a test artifact or inject a runtime configuration without configuring complex networking, they will abandon the tool. By providing a zero-configuration, secure file transfer mechanism over virtio-vsock, we dramatically improve the "Time to Value" for developers, reducing debugging time and increasing adoption as a developer tool.

## Gap Analysis
While we have implemented virtio-vsock for metrics (Phase 12), we only use it for small JSON/MessagePack payloads. We currently lack a generic file transfer protocol over this socket. Standard tools like `scp` or `rsync` require a network stack (TCP/IP) and an SSH daemon, which violate our minimal initramfs philosophy. We need a purpose-built file copy mechanism that operates directly over the hypervisor socket.

## Acceptance Criteria
- 👤 **User Story:** As a Developer, I want to copy a binary from my host machine into the running microVM, so that I can test it without configuring complex networking or SSH.
- 👤 **User Story:** As a CI System, I want to extract the `test-results.xml` file from the guest back to the host, so that the pipeline can report pass/fail metrics.
- ✅ **Metric Definition:** Success = A 100MB file can be transferred from host to guest (and vice versa) in under 2 seconds without corrupting data or requiring external network setup.
- **Functional Requirements:**
  - Add CLI commands `hitz vm cp <src> <vm_id>:<dest>` and `hitz vm cp <vm_id>:<src> <dest>`.
  - The daemon must negotiate the file transfer over an established virtio-vsock channel.
  - The guest agent must handle the file creation, writing, reading, and permission management securely.
  - The transfer must stream the file in chunks to prevent memory exhaustion on large files.

## 🚫 Out of Scope
- **Directory Sync / Rsync equivalent:** We are building a simple file copier, not a continuous synchronization engine.
- **Symlink resolution:** Only regular files will be transferred. Symlinks will be ignored or result in an error.
- **File System Mounting:** Mounting a host directory into the guest (like virtiofs) is a separate, much larger feature. This is strictly a push/pull copy mechanism.
