# 🔭 Vantage: Spec for Accurate Process CPU Metrics
## Problem Statement
Currently, the guest agent reports process CPU usage as a lifetime average (`(lifetime_ticks / USER_HZ) / uptime_secs * 100`). This means a process that spiked to 100% CPU an hour ago but is now idle will still show a non-zero, slowly decaying CPU percentage.
## The "So What?"
What business problem does this solve? Observability accuracy. When operators view the metrics output or an external OTel dashboard, they expect to see the *current* CPU usage of a process (e.g., over the last 1-2 seconds), not a historical average since the VM booted. Inaccurate metrics lead to false alarms and make it impossible to identify which process is currently causing a CPU spike in production.
## Gap Analysis
- **Current State:** The process CPU calculation in `hitz-guest-agent` computes `cpu_pct` using the process's lifetime CPU ticks divided by the VM's total uptime ticks. There is a code comment noting: `Future: implement two-sample differential for accurate current usage.`
- **Market Standard:** Standard top utilities and monitoring agents calculate CPU usage by taking two samples over a known time window and measuring the delta in ticks.
## Acceptance Criteria
- 👤 **User Story:** As an Operator, I want the VM metrics to show the current CPU percentage of processes, so that I can accurately identify which process is currently consuming resources.
- ✅ **Metric Definition:** Success = A process that sleeps for 10 seconds and then spikes to 100% CPU shows ~0% usage during sleep and ~100% usage during the spike in the metrics snapshot, rather than a lifetime average.
## 🚫 Out of Scope
- Modifying the host-side metrics API or CLI.
