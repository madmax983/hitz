# 🔭 Vantage: Spec for Virtio-FS (Shared Folder) Support

## Problem Statement
Currently, Hitz supports standard block devices for persistent storage via `virtio-blk`. However, sharing files between the Windows host and the Linux microVM requires network-based solutions like SSH/SCP or setting up a complex Samba/NFS server. There is no built-in, low-latency mechanism to seamlessly mount a host directory inside the microVM.

## The "So What?"
What business problem does this solve? For developer environments and container-like workflows, seamless host-guest file sharing is a critical feature. Developers need to edit code on their Windows host using their IDE of choice, and have those changes instantly reflect inside the microVM running the application. Implementing Virtio-FS eliminates the network overhead and complex configuration, significantly improving the local development experience and making Hitz a viable backend for desktop container tools (e.g., Docker Desktop equivalents).

## Gap Analysis
Our current I/O architecture supports `virtio-blk` for block storage and `virtio-net` for networking. We lack a `virtio-fs` implementation (which relies on the vhost-user-fs protocol or an embedded FUSE server) in our `hitz-devices` and `hitz-vmm` crates. The API also lacks configuration fields to specify host directory paths and their corresponding guest mount tags.

## Acceptance Criteria
- **User Story:** As a Developer, I want to map a folder from my Windows host into my Linux microVM, so that I can edit source files natively on Windows and execute them immediately in the Linux environment without manual copying.
- **Metric Definition:** Success = A user can specify a host directory and a tag in the VM configuration. The microVM boots, and the directory can be mounted via `mount -t virtiofs <tag> /mnt` inside the guest with read/write access and sub-millisecond latency.
- **Functional Requirements:**
  - Add a `shared_folders` field to the `VmConfig` REST API.
  - Implement a `virtio-fs` device in `hitz-devices` that acts as a bridge to the Windows host filesystem.
  - The implementation must properly map Windows file permissions and paths to their Linux equivalents.

## Out of Scope
- **Live Migration of Shared State:** Snapshotting or live-migrating a VM with an active Virtio-FS mount is complex and out of scope for the initial implementation.
- **Symlink Support to Host-only Paths:** Resolving symlinks that point outside the exported host directory is considered a security risk and will be explicitly denied.
