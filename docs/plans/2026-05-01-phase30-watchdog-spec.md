# 🔭 Vantage: Spec for Guest Watchdog & Auto-Recovery

## Problem Statement
Currently, our VMs emit telemetry, but if a guest kernel panics or the agent deadlocks, the VM remains in a "zombie" state indefinitely. Operators are forced to build external alerting and manual restart playbooks.

## The "So What?"
What business problem does this solve? This increases operational toil and severely impacts SLA compliance. By implementing an automated watchdog, we reduce operational cost and increase reliability, which is a key selling point for production workloads.

## Gap Analysis
- **Current State:** We pull metrics manually and push them to OTel. There is no feedback loop back into the VM lifecycle management.
- **Market Standard:** Kubernetes has liveness probes. AWS EC2 has instance status checks and auto-recovery. Proxmox/vSphere have HA watchdogs. Our micro-VMs currently lack this fundamental reliability feature.

## Acceptance Criteria
- 👤 **User Story:** As a Site Reliability Engineer (SRE), I want the system to automatically detect and restart unresponsive micro-VMs, so that service uptime is maintained without manual intervention and MTTR is minimized.
- ✅ **Metric Definition:** Success = Time to detect an unresponsive VM and initiate recovery is < 30 seconds for 99% of failures. False positive rate for restarts is < 0.1% under heavy load.
- **Functional Requirements:**
  - Must support configuring a timeout threshold per VM.
  - Must cleanly force-stop and recreate the VM if the watchdog timeout is breached.
  - Must emit an event/log when a watchdog recovery is triggered, clearly distinguishing it from a normal user-initiated restart.
  - Must prevent "restart loops" (e.g., if a VM crashes repeatedly within a short window, it should eventually be marked as failed and back off).

## 🚫 Out of Scope
- Implementation details such as the specific communication channel (e.g., vsock vs. serial).
- Complex HTTP health checks (e.g., checking specific HTTP endpoints inside the guest). This is purely a basic guest liveness check.
- Live migration of degraded VMs to other hosts.
