# 🔭 Vantage: Spec for Guest State Inspection API

## Problem Statement
Currently, our virtual processor implementation has a gap in its interface for retrieving special registers. Because of this, developers and external tooling cannot fully inspect the deep architectural state of a running or halted virtual machine. This severely hinders advanced debugging, security analysis, and integration with future features like live migration.

## The "So What?"
What business problem does this solve? Without the ability to read the full architectural state (including special registers like segment selectors and control registers), operators are flying blind when a guest kernel crashes in production. By implementing a full Guest State Inspection API, we empower SREs and security researchers to perform live forensics and memory dumps. This turns Hitz from a basic virtualization tool into a mature platform suitable for enterprise diagnostics and security-critical workloads.

## Gap Analysis
- **Current State:** The virtual processor components contain missing implementations for retrieving special registers, preventing any external caller from extracting crucial guest CPU control states.
- **Market Standard:** Enterprise hypervisors (like QEMU/KVM via `info registers` and Firecracker's microVM snapshots) allow operators to retrieve full CPU state. We currently lack this fundamental observability primitive.

## Acceptance Criteria
- 👤 **User Story:** As an SRE, I want to inspect the special CPU registers of a paused micro-VM, so that I can diagnose kernel panics and page faults without having to attach a full debugger.
- 👤 **User Story:** As a Security Auditor, I want to programmatically read the control registers (e.g., CR3) of a guest, so that I can securely audit memory layout and page tables from the host side.
- ✅ **Metric Definition:** Success = A user executes `hitz vm inspect <vm_id> --registers special` and receives a JSON-formatted dump of all special registers (CR0, CR3, CR4, segment descriptors) in under 50ms without panicking the VM daemon.
- **Functional Requirements:**
  - Implement the necessary hypervisor platform-specific backend calls to retrieve special registers.
  - Expose a new API endpoint in `hitz-daemon` for fetching full CPU state.
  - Implement a CLI command (`hitz vm inspect`) to trigger the state dump and output it cleanly.
  - The implementation must safely handle querying the state while the VCPU is paused or actively running (by injecting a VM exit).

## 🚫 Out of Scope
- **Writing Special Registers:** This phase strictly covers *reading* special registers, not *writing* or modifying the guest's running state.
- **Full Memory Dumps:** Inspecting guest RAM is out of scope; this is purely for the CPU's architectural register state.
