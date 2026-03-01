# ADR-001: Use WHP (Windows Hypervisor Platform) API

## Status

Accepted

## Context

We need a hypervisor API on Windows for building a micro-VM manager.
Options considered:

1. **WHP (Windows Hypervisor Platform)** — Low-level API, similar to KVM ioctls
2. **Hyper-V WMI** — High-level management API, COM-based
3. **Hyper-V PowerShell** — Automation layer over WMI

## Decision

Use WHP via the `windows-rs` crate.

## Rationale

- WHP provides full VMM control: partition creation, memory mapping, vCPU register access, run loop
- Closest analog to KVM, which Firecracker uses — our architecture reference
- Direct FFI through `windows-rs` (official Microsoft crate)
- No COM overhead or WMI dependency

## Consequences

- Must handle MMIO instruction decoding ourselves (via WinHvEmulation.dll)
- No `ioeventfd`/`irqfd` — interrupt delivery requires cancel + interrupt window pattern
- More code than using Hyper-V WMI, but more control
- Windows-only; future KVM backend possible through `hitz-hal` trait abstraction
