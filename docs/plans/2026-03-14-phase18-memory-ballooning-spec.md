# 🔭 Vantage: Spec for Memory Ballooning (virtio-balloon)

## Problem Statement
Currently, a Hitz microVM claims its fully configured RAM capacity upfront (e.g., 2GB). Even if the guest OS only uses 200MB of that allocation, the host cannot easily reclaim the unused 1.8GB. In high-density serverless or container orchestration environments, this static allocation prevents efficient over-provisioning of host memory, severely limiting the number of microVMs a single node can host.

## The "So What?"
Cloud providers and enterprise infrastructure operators need to maximize their hardware ROI (Return on Investment). By reclaiming unused guest memory and returning it to the host OS, administrators can safely overcommit physical RAM. Without memory ballooning, Hitz forces operators to under-utilize their hardware or risk out-of-memory (OOM) crashes on the host, increasing the cost-per-VM and making the product uncompetitive against legacy hypervisors or container runtimes.

## Gap Analysis
Industry standard hypervisors (like KVM/QEMU, Hyper-V, and Firecracker) support dynamic memory management via the `virtio-balloon` device (or equivalent integration services). Without this feature, our platform lacks the dynamic resource elasticity required for modern, high-density cloud native workloads.

## Goal
Implement the `virtio-balloon` device to allow the host to dynamically inflate (reclaim memory from the guest) and deflate (return memory to the guest) a memory balloon within running microVMs, enabling safe host memory oversubscription.

## Scope

**In Scope:**
- Expanding the API to support dynamic `AdjustMemory` or `SetBalloonSize` actions on a running VM.
- Adding a `virtio-balloon` backend device implementation to `hitz-devices/src/virtio/`.
- Integrating the balloon device with the guest's virtio subsystem via the MMIO transport.
- Safely handling guest memory pages returned by the balloon (e.g., using `DiscardVirtualMemory` or `VirtualFree` equivalent on Windows to return physical pages to the host OS while preserving the guest physical address space mapping).

**Out of Scope:**
- Automatic, host-wide memory balancing daemons (this relies on the user/orchestrator making API calls).
- Guest memory statistics reporting (often bundled with ballooning, but deferred to a separate observability spec for simplicity).
- Page compression or swapping to disk (zRAM/swap).

## Acceptance Criteria
- **User Story:** As an Infrastructure Operator, I want to dynamically reclaim unused memory from a running microVM so that I can launch additional workloads on the same physical server.
- **Metric:** The `SetBalloonSize` API call must successfully instruct the guest to release memory, and the host OS must reflect the reclaimed physical RAM within 2 seconds. The guest must not panic or OOM if the balloon target is within safe limits.
- **Functionality:**
  - The CLI and API must support an `adjust-memory` action on a running VM.
  - The `virtio-balloon` implementation must correctly process inflate and deflate queue requests from the guest.
  - The host must genuinely recover physical RAM pages (the working set size of the `hitz-daemon` process should demonstrably shrink when the balloon inflates).

## Proposed Additions

### CLI Updates
```bash
# Instruct the guest to inflate the balloon to 1GB, reclaiming it for the host
hitz vm adjust-memory my-vm --target-mb 1024
```

### API Updates
`PATCH /api/v1/vms/{id}/memory`
```json
{
  "action": "SetBalloonSize",
  "target_bytes": 1073741824
}
```
