# 🔭 Vantage: Spec for Pre-warmed VM Pools

## Problem Statement
Cold starts for microVMs currently take too long (often seconds) when full boot sequences, OS initialization, and runtime loading occur on the critical path of a user request.

## The "So What?"
What business problem does this solve? For serverless and function-as-a-service (FaaS) workloads, latency is revenue. If an API request triggers a cold start, user experience degrades significantly. By maintaining a pool of pre-warmed VMs, we eliminate boot latency from the request path, enabling sub-millisecond execution times and making Hitz competitive for high-throughput, low-latency edge computing.

## Gap Analysis
Leading serverless runtimes (like Firecracker/AWS Lambda) utilize snapshotting or pre-warmed VM pools to mask boot latency. Hitz currently boots every VM from scratch on demand, lacking the capability to hold idle, booted instances ready for immediate workload injection.

## Acceptance Criteria
- 👤 **User Story:** As a Serverless Platform Operator, I want to maintain a pool of pre-booted VMs, so that incoming functions can be executed with zero cold-start penalty.
- ✅ **Metric Definition:** Success = Pre-warmed VMs transition from "idle" to "executing payload" in < 5ms for 99% of requests.
- **Functional Requirements:**
  - The daemon must support configuring a minimum pool size of pre-warmed VMs.
  - Pre-warmed VMs must pause execution just before jumping into the user workload, minimizing CPU usage.
  - A fast-path mechanism must be provided to inject workload configuration/data into a pre-warmed VM and resume it.
  - The system must automatically replenish the pool in the background as VMs are consumed.

## 🚫 Out of Scope
- Snapshot/Restore based pooling (this focuses on live, paused VMs).
- Automated scaling based on predictive request metrics.
