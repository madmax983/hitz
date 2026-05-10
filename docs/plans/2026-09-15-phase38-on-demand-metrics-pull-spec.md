# 🔭 Vantage: Spec for On-Demand Metrics Pull

👤 **User Story:** As an Observability Engineer, I want to request real-time metrics on-demand via an API call, so that I can immediately inspect the health and resource utilization of a specific VM during an incident without waiting for the next automated push cycle.

✅ **Acceptance Criteria:**
- **Metric Definition:** Success = Invoking the metrics endpoint for a specific VM returns the latest metrics payload within 500ms, triggering an active fetch from the guest agent rather than returning cached data.
- **So What? (Business Problem):** Rapid incident response. While periodic push metrics are great for long-term trends and dashboards, live troubleshooting requires immediate feedback. If an operator suspects a VM is struggling, they need to query its state *now*. Without on-demand pull, time-to-resolution increases as operators wait for the next push interval.
- **Gap Analysis:** Currently, the system only supports a periodic push path. The programmatic API for requesting a metrics snapshot is a stub and returns no data (as noted by the "Future: implement pull by injecting a request packet" comment in the codebase). We need a mechanism to actively request and await a metrics response from the guest.

🚫 **Out of Scope:**
- **Historical Metrics Querying:** This feature strictly fetches the *current* live state. Querying past metrics remains the responsibility of the external observability platform.
