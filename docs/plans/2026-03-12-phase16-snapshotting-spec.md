# 🔭 Vantage: Spec for VM Snapshotting (Suspend/Resume)

## Problem Statement
Booting a microVM from scratch, even quickly, incurs OS initialization overhead. For high-density, scale-to-zero serverless environments, latencies must be in the single-digit milliseconds. Currently, Hitz cannot pause a running VM and resume it later, or duplicate a "warmed up" VM state to multiple new instances instantly.

## The "So What?"
Users want serverless-like cold start times (< 10ms). Without snapshotting (saving the exact CPU and memory state of a running VM to disk), Hitz forces users to pay the full guest OS boot tax on every invocation. Snapshotting turns compute into a cheap, resumable commodity, unlocking true serverless economics and instant horizontal scaling.

## Gap Analysis
Industry standard microVM hypervisors (like Firecracker) natively support microVM snapshotting. Without this feature, our platform lags behind competitors in terms of serverless "cattle" economics and cold-start performance.

## Goal
Enable saving a running VM's entire state (Memory and vCPU registers) to disk, and restoring from that state on demand, bypassing the guest OS boot process.

## Scope

**In Scope:**
- Pausing a running VM.
- Serializing vCPU state and guest physical memory to a snapshot file.
- Creating a new VM by resuming from a snapshot file.
- Updating configuration APIs to accept a snapshot source.

**Out of Scope:**
- Live migration (moving a running VM across physical hosts over a network).
- Snapshotting network connections or complex stateful external devices (e.g., active TCP sessions might drop, which the guest must handle).

## Acceptance Criteria
- **User Story:** As a Serverless Platform Operator, I want to snapshot a "warmed up" VM so that I can instantly clone and resume it multiple times to handle burst traffic with < 10ms latency.
- **Metric:** The `Resume` operation from a snapshot file located in RAM disk must take < 10ms to begin guest code execution.
- **Functionality:**
  - The CLI and API must support a `snapshot` action on a running VM.
  - The CLI and API must support a `resume` action (or `create` from snapshot).
  - A resumed VM must continue execution from the exact instruction where it was snapshotted.
  - The snapshot format must be portable across the same CPU architecture.

## Proposed Additions

### CLI Updates
```bash
# Save the state
hitz vm snapshot my-vm --output ./warm.snap

# Resume a new instance from that state
hitz vm create new-vm --snapshot ./warm.snap
```
