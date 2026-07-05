# Hitz 🎻

## Abstract
Hitz is a modern, Rust-based micro-VM manager and hypervisor ecosystem designed to run lightweight virtual machines safely and efficiently. By leveraging virtualization APIs and a strictly modular design, it bridges the gap between hardware execution and human-friendly orchestration. If you need a fast, isolated environment for your workloads, Hitz is your orchestrator.

## The Hero's Journey
Getting a micro-VM off the ground requires a blueprint. With Hitz, you define the resources and let the ecosystem handle the heavy lifting:

```rust
// A typical configuration blueprint for a new VM
use hitz_api::{VmConfig, GuestAgentMode};
use std::path::PathBuf;

let config = VmConfig {
    kernel_path: PathBuf::from("/boot/vmlinux"),
    initramfs_path: Some(PathBuf::from("/boot/init.cpio")),
    disk_path: None,
    ram_mib: 1024,
    cpus: 4,
    cmdline: Some("console=ttyS0 quiet".to_string()),
    net: None,
    ports: vec![],
    guest_cid: 3,
    guest_agent: GuestAgentMode::Auto,
};
// This configuration is serialized and sent to the hitz-daemon to spawn the VM!
```

## The Fine Print
The Hitz workspace is composed of tightly cohesive, decoupled crates to ensure maximum flexibility and safety:

- `hitz-hal`: Platform-agnostic Hypervisor Abstraction Layer.
- `hitz-whp`: Windows Hypervisor Platform backend implementation.
- `hitz-vmm`: Virtual Machine Monitor core loop and memory management.
- `hitz-boot`: Linux kernel loader, ACPI tables, and initramfs configuration.
- `hitz-devices`: Emulated hardware and virtio devices (e.g., vsock, net, block).
- `hitz-net`: Virtual networking and interface management.
- `hitz-api`: REST API types, configuration blueprints, and telemetry definitions.
- `hitz-daemon`: Background service that orchestrates VM lifecycles and handles requests.
- `hitz-cli`: Command-line interface for operators to interact with VMs.
- `hitz-guest-agent`: Telemetry agent running inside the guest OS to report health and usage.
