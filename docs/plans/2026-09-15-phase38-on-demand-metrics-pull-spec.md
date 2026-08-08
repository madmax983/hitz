# Spec: On-Demand Metrics Pull

👤 **User Story:** As an Operator, I want to request real-time metrics from a running microVM synchronously, so that I can immediately diagnose live incidents without waiting for the next scheduled telemetry push.

✅ **Acceptance Criteria:**
- **Metric Definition:** Success = Querying the API for a running VM's metrics successfully returns a fresh metrics payload (CPU, memory, disk, network) within 1 second of the request.
- **So What? (Business Problem):** While our automated push-to-OpenTelemetry stream is excellent for long-term observability dashboards, live incident response demands immediate, synchronous feedback. When an alert fires, operators need to see the exact state of the VM *right now* to diagnose CPU spikes or memory exhaustion. Without an on-demand pull mechanism, incident resolution is delayed by the telemetry polling interval, increasing Mean Time to Resolution (MTTR) and operational cost.
- **Gap Analysis:** The daemon currently supports push-based telemetry to OTel via vsock channels, but on-demand pull is not yet implemented (it currently returns `None`). Future work requires injecting a request packet and awaiting the response from the guest agent via a dedicated pull channel separate from the push stream.

🚫 **Out of Scope:**
- Modifying or replacing the existing push-based OpenTelemetry (OTel) metrics pipeline.
- Storing or aggregating historical metrics in the local state store.
- Altering the metrics data format (we will use the existing JSON snapshot format).
