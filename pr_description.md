🔭 Vantage: Spec for VCPU Cancellation API

👤 **User Story:** As a VMM Operator, I want to send a cancellation signal to a running VCPU, so that I can gracefully stop the micro-VM without data loss.

✅ **Acceptance Criteria:**
- Success = A running VM can be sent a cancellation request, causing the VCPU loop to exit with `VcpuExit::Canceled` within 10ms, allowing a clean shutdown.
- Replace `unimplemented!()` in `cancel_handle` within `crates/hitz-vmm/src/run_loop.rs` (and associated traits) with the correct WHP platform-specific backend calls.
- Expose a mechanism to safely trigger cancellation from another thread.
- Ensure the VCPU loop correctly handles the cancellation exit and cleans up resources.

🚫 **Out of Scope:** Live Migration: Pausing for live migration is out of scope; this is purely for graceful shutdown.
