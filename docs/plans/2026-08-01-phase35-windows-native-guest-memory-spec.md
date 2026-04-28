# 🔭 Vantage: Spec for Windows-Native Guest Memory Allocator

## Problem Statement
Currently, the `vm-memory` crate (from the rust-vmm project) is heavily Linux-focused and fails to compile cleanly or operate correctly on Windows due to POSIX assumptions. This prevents us from effectively allocating, mapping, and managing guest physical memory when running Hitz natively on Windows. We lack a purpose-built, Windows-native memory abstraction.

## The "So What?"
What business problem does this solve? Reliable memory management is the foundation of any hypervisor. If we cannot reliably allocate and map guest memory on Windows without relying on non-portable or failing Linux abstractions, we cannot deliver a stable product on our primary target platform. Building a custom guest memory implementation using native Windows APIs ensures stability, optimal performance, and eliminates dependency on upstream Linux-centric crates. This guarantees our core platform works flawlessly on Windows.

## Gap Analysis
- **Current State:** We are attempting to use or work around the `vm-memory` crate, which is incompatible with Windows.
- **Market Standard:** Native hypervisors (like Hyper-V) manage guest physical addresses by directly mapping host virtual addresses allocated via platform-native APIs. We currently lack this Windows-native mapping layer in our abstractions.

## Acceptance Criteria
- 👤 **User Story:** As a Core VMM Developer, I want a Windows-native guest memory abstraction, so that I can reliably map RAM into the microVM partition without compilation errors or POSIX compatibility issues.
- ✅ **Metric Definition:** Success = The VMM successfully allocates a 1GB block of memory using native platform primitives, maps it into the guest physical address space, and successfully boots a Linux guest kernel into user-space without any memory corruption or panics.
- **Functional Requirements:**
  - Remove the failing dependency on the `vm-memory` crate.
  - Implement a custom guest memory management abstraction.
  - The implementation must use appropriate page-alignment to allocate host virtual memory using native Windows capabilities.
  - The implementation must use the native Windows Hypervisor Platform APIs to map the host virtual memory to the guest physical addresses.
  - Provide standard memory read/write accessors for the host to interact with guest memory (e.g., for loading the kernel and initramfs).

## 🚫 Out of Scope
- **Memory Ballooning:** Dynamic resizing or reclaiming of guest memory is out of scope for this phase. The memory allocation will be static at boot time.
- **NUMA Awareness:** Complex NUMA node-aware allocations are out of scope. Basic contiguous virtual allocations will suffice.
