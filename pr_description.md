Title: 🔭 Vantage: Spec for Windows-Native Guest Memory Allocator

👤 **User Story:** As a Core VMM Developer, I want a Windows-native guest memory abstraction, so that I can reliably map RAM into the microVM partition without compilation errors or POSIX compatibility issues.

✅ **Acceptance Criteria:**
- Must remove dependency on Linux-centric `vm-memory` crate.
- Must allocate memory using native Windows capabilities.
- Must map memory to the guest using native Windows Hypervisor Platform APIs.
- Must provide standard read/write accessors for guest memory.

🚫 **Out of Scope:** Memory Ballooning and NUMA awareness.

**Note:** Tested `hitz-api` individually on Linux since the workspace fails on Linux due to Windows-specific dependencies (e.g. `wintun`). This PR only introduces a documentation specification and doesn't modify application code.
