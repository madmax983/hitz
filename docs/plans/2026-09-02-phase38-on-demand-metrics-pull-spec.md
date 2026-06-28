# 🔭 Vantage: Spec for On-Demand Metrics Pull

## Problem Statement
Currently, Hitz relies entirely on a "push" model where the guest agent periodically sends metrics to the host (e.g., every 5 seconds). If a platform operator needs the current state immediately to diagnose a sudden load spike or verify a configuration change, they must wait for the next periodic broadcast.

## The "So What?"
What business problem does this solve? Incident response and autoscale triggers require real-time data. Waiting up to 5 seconds for a periodic push introduces unacceptable latency when making automated scaling decisions or debugging live issues. By allowing the host to explicitly request and await an immediate metrics payload from the guest, we reduce observability latency to milliseconds, enabling tighter autoscaling loops and faster Mean Time to Resolution (MTTR) during incidents.

## Gap Analysis
- **Current State:** The daemon only supports receiving passive periodic metric pushes from the guest. The API to actively pull metrics is stubbed out and returns empty data. The guest agent is capable of responding to requests, but the host lacks the ability to initiate them.
- **Market Standard:** Leading container and virtualization observability platforms allow both interval-based scraping and on-demand polling of agent health and utilization metrics.

## Acceptance Criteria
- **User Story:** As a Platform Operator, I want to actively request a real-time metrics snapshot from a specific microVM, so that I can immediately diagnose performance anomalies without waiting for the next periodic push interval.
- **Metric Definition:** Success = An API request to pull metrics returns a complete metrics payload within 50ms for a healthy VM, measured from the daemon API edge.
- **Functional Requirements:**
  - The host daemon must be able to send a request packet to the guest agent over the existing communication channel.
  - The guest agent must recognize this request and immediately generate and return fresh metrics rather than waiting for its normal sleep cycle.
  - The daemon must await this response and serve it to the API caller.

## 🚫 Out of Scope
- **Historical Metrics Storage:** This feature only provides the current, real-time snapshot. Time-series aggregation and historical tracking remain the responsibility of external observability systems.
- **Guest Agent Upgrades:** Deploying new agent binaries to old VMs is out of scope.
