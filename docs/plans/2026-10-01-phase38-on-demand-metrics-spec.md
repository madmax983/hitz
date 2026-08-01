# 🔭 Vantage: Spec for On-Demand Guest Metrics Pull API

## Problem Statement
Currently, the Hitz guest agent only supports pushing metrics on a fixed interval (e.g., every 10 seconds) to a pre-configured endpoint. The API has a stub for an on-demand pull feature, but it is currently hardcoded to return no data, with a note that it is a future enhancement. Operators cannot fetch real-time metrics on demand.

## The "So What?"
What business problem does this solve? When debugging a live incident (e.g., high latency or CPU spikes), operators need real-time data immediately. Waiting up to 10 seconds for the next automated metrics push is unacceptable during an active outage. By implementing the on-demand pull API, we provide instantaneous observability, drastically reducing the Time to Resolve (TTR) for production incidents and improving operator trust in the platform.

## Gap Analysis
- **Current State:** The on-demand pull path is an unimplemented stub. The push-based metrics work, but the pull thread does not correctly trigger and respond to on-demand requests.
- **Market Standard:** Modern hypervisors and container runtimes (like Docker `stats` or Kubernetes `kubectl top`) allow instantaneous, on-demand metrics retrieval via their CLI and API.

## Acceptance Criteria
- 👤 **User Story:** As an Operator, I want to query a microVM's metrics on demand via the CLI or API, so that I can instantly check its health without waiting for the next automated push interval.
- ✅ **Metric Definition:** Success = A user executes a CLI command to retrieve metrics, which triggers the daemon to pull the latest data from the guest. The user receives a JSON-formatted metrics payload (CPU, memory, disk) in under 100ms.
- **Functional Requirements:**
  - Implement the daemon-side logic to send a pull request to the guest agent.
  - Ensure the guest agent's pull thread correctly listens for requests and responds with the latest metrics snapshot.
  - The CLI and API must expose a synchronous endpoint for this on-demand retrieval.

## 🚫 Out of Scope
- **Modifying the Guest Agent Collection Logic:** We will reuse the existing metrics collection logic.
- **Continuous Streaming:** Streaming metrics over a persistent API connection is out of scope; this is for discrete, point-in-time on-demand requests.
