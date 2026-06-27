# 🔭 Vantage: Spec for Accurate Per-Process CPU Metrics

## Problem Statement
Currently, the guest agent calculates per-process CPU usage as a lifetime average (total ticks consumed divided by total system uptime). This means that a process which spiked CPU usage hours ago but is currently idle will still show an artificially high CPU percentage. This inaccurate reporting misleads autoscalers, health checks, and dashboard metrics.

## The "So What?"
What business problem does this solve? Observability accuracy. When orchestrators (like Kubernetes or custom rightsizers) rely on our metrics API to make scaling or termination decisions, they need to know what the process is doing *right now*, not what it did yesterday. Inaccurate metrics lead to over-provisioning (wasted money) or under-provisioning (customer outages). Accurate two-sample differential metrics are required for production-grade telemetry.

## Gap Analysis
- **Current State:** The agent reads `/proc/[pid]/stat` once per collection frame and computes the lifetime average. There is an explicit `// Future: implement two-sample differential for accurate current usage.` comment in `crates/hitz-guest-agent/src/main.rs`.
- **Market Standard:** Tools like `top`, `htop`, and `datadog-agent` compute CPU usage by taking a sample, waiting a specific interval, taking a second sample, and calculating the delta. Our system currently falls short of this industry standard.

## Acceptance Criteria
- **User Story:** As an Operator, I want the per-process CPU percentage to reflect the usage within the current polling window, so that my autoscaler makes accurate scaling decisions based on current load.
- **Metric Definition:** Success = A process that sleeps for 59 seconds and consumes 100% CPU for 1 second reports ~1.6% CPU usage over a 60-second polling interval, rather than an infinitely compounding lifetime average.

## 🚫 Out of Scope
- **Cgroups Analysis:** This phase focuses only on basic PID metrics. Reading from `cgroup` is deferred to the OCI container support phase.
