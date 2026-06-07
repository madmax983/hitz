# 🔭 Vantage: Spec for Simulated Device I/O Exits

## Problem Statement
Currently, the `hitz-vmm` mock virtual processor implementation (`DummyVcpu`) has an `unimplemented!()` catch-all match arm for handling virtual CPU exits. Because of this, developers writing tests or simulating device interactions cannot reliably emulate complex I/O behaviors (like specific MMIO or port I/O patterns) without crashing the test harness.

## The "So What?"
What business problem does this solve? Testing reliability and developer velocity. Without the ability to simulate comprehensive device I/O exits during tests, regressions in device emulation logic (like `virtio` processing) might slip through. Implementing full support for simulated exits in the test framework ensures that we can write robust, comprehensive integration tests for all device drivers, reducing the risk of production bugs and accelerating feature development.

## Gap Analysis
- **Current State:** The mock VCPU run loop contains an `unimplemented!()` stub for several `VcpuExit` variants (e.g., `Mmio`, `IoPort`), preventing tests from simulating these events.
- **Market Standard:** Mature hypervisor test frameworks provide robust mock implementations that accurately simulate all possible guest interactions, enabling exhaustive unit testing of the VMM.

## Acceptance Criteria
- 👤 **User Story:** As a VMM Developer, I want to simulate specific MMIO and I/O port exits in unit tests, so that I can verify the VMM's device emulation logic handles them correctly without crashing.
- ✅ **Metric Definition:** Success = A new unit test configuring the `DummyVcpu` to return an `Mmio` or `IoPort` exit successfully simulates the interaction, and the VMM handles the exit without hitting an `unimplemented!()` panic.
- **Functional Requirements:**
  - Update the `DummyVcpu` implementation in `crates/hitz-vmm/src/run_loop.rs` to handle all `VcpuExit` variants defined in `hitz-hal`.
  - Provide a mechanism to inject simulated exits into the mock VCPU.

## 🚫 Out of Scope
- **Production Implementation:** This phase strictly covers the mock implementation used for testing (`DummyVcpu`). It does not alter the production WHP execution loop.
