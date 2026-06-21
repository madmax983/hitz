# 🔭 Vantage: Spec for Mock VCPU I/O Support

## Problem Statement
Currently, the mock virtual CPU used in our testing framework contains an `unimplemented!()` stub for handling MMIO and I/O Port exits. Because of this, developers cannot write isolated unit tests that simulate bidirectional I/O between the mock CPU and virtual devices without panicking.

## The "So What?"
What business problem does this solve? Developer velocity and product reliability. Without the ability to simulate I/O operations in our mock CPU, testing virtual devices requires spinning up full, heavy-weight virtual machines with a real hypervisor backend. This makes our CI pipeline slow, flaky, and expensive. By implementing I/O support in the mock CPU, we empower engineers to write lightning-fast, deterministic unit tests.

## Gap Analysis
- **Current State:** The testing mock for the virtual CPU contains an `unimplemented!()` stub for handling I/O and MMIO exits, preventing it from successfully simulating I/O operations.
- **Market Standard:** Modern hypervisor test suites provide fully-featured mock CPUs to test device emulation logic; we currently lack this foundational testing primitive.

## Acceptance Criteria
- 👤 **User Story:** As a Core Engineer, I want the mock virtual CPU to support returning I/O exits, so that I can write isolated unit tests for virtual devices without needing a real hypervisor backend.
- ✅ **Metric Definition:** Success = Engineers can write and execute unit tests that simulate bidirectional I/O (MMIO and IoPort) between the mock CPU and a virtual device, with the execution loop correctly returning the simulated I/O exit instead of panicking with `unimplemented!()`.
- **Functional Requirements:**
  - The mock CPU must be able to gracefully return MMIO and I/O Port exits.
  - The testing framework must provide a way to script a sequence of expected I/O operations and assertions.

## 🚫 Out of Scope
- **Real Device Emulation:** This is purely for simulating exits in a testing context, not emulating real devices within the mock CPU.
- **Advanced CPU Features:** Support for advanced architectural features (e.g., paging, floating point) is out of scope.
