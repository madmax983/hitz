# 🔭 Vantage: Spec for Accurate Process CPU Metrics

## Problem Statement
Currently, the guest agent calculates process CPU usage as a lifetime average since boot. This means short, intense CPU spikes in long-running processes are heavily diluted and effectively hidden from observability tools. The `hitz-guest-agent/src/main.rs` file explicitly notes this gap as a "Future" item.

## The "So What?"
What business problem does this solve? Observability must be actionable. When an operator asks "what process is currently spiking the CPU," a lifetime average gives them an incorrect answer. Implementing a two-sample differential (e.g., over a 500ms window) for process-level metrics provides accurate, real-time resource attribution, enabling effective alerting and root-cause analysis.

## Gap Analysis
- **Current State:** `hitz-guest-agent` computes process CPU percentage using total ticks divided by system uptime.
- **Market Standard:** Standard tools (like `top`, `htop`, and the Datadog Agent) calculate process CPU usage differentially over a sampling window (e.g., delta ticks / delta time) to reflect current reality.

## Acceptance Criteria
- 👤 **User Story:** As an Operator, I want to see the current CPU percentage of a guest process over the last 500ms window, so that I can accurately identify processes currently spiking the CPU.
- ✅ **Metric Definition:** Success = Executing `hitz vm metrics <vm_id>` correctly attributes a short CPU spike to the offending process with >90% accuracy, matching the output of `top` run concurrently within the guest.
- **Functional Requirements:**
  - Modify `hitz-guest-agent` to take an initial snapshot of `/proc/*/stat` for all active processes, wait for a defined sampling window (e.g., 500ms), and then take a second snapshot.
  - Calculate the CPU percentage for each process using the difference in `utime` and `stime` across the two samples, proportional to the elapsed time.

## 🚫 Out of Scope
- **Host-Level Metrics:** This strictly applies to process-level CPU attribution inside the guest. Overall guest CPU utilization (which already uses a two-sample diff) is out of scope.
- **Persistent Metric Storage in Guest:** Storing historical metrics inside the guest agent memory beyond the immediate 500ms sampling window is out of scope.
