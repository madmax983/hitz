# 🔭 Vantage: Spec for UDP Port Forwarding

## Problem Statement
Currently, Hitz supports forwarding host TCP ports to guest microVMs, enabling seamless access to web servers, databases, and SSH. However, it completely lacks support for UDP port forwarding. Users running UDP-based workloads—such as DNS servers (e.g., CoreDNS), game servers, VPN endpoints (e.g., WireGuard), or metrics collectors (e.g., StatsD/syslog)—cannot expose these services to the outside world or the host machine.

## The "So What?"
What business problem does this solve? Modern cloud-native and edge environments rely heavily on UDP for low-latency communication and specialized protocols (DNS, QUIC, WebRTC). By not supporting UDP port forwarding, we lock out a massive category of edge workloads and network appliances. Implementing UDP support expands the utility of Hitz, making it a viable target for running comprehensive network functions and modern web applications that utilize HTTP/3, directly increasing platform adoption.

## Gap Analysis
Our current implementation explicitly targets TCP-only forwarding. The configuration API and the CLI (`--port 2222:22`) do not distinguish between protocols, implicitly assuming TCP. We lack a mechanism to specify the protocol (TCP vs UDP) in the API, and we lack the data-plane proxying logic to handle bidirectional UDP datagrams between the host and the guest network stack.

## Acceptance Criteria
- 👤 **User Story:** As an Edge Infrastructure Engineer, I want to forward UDP traffic from my host machine to a DNS server running inside the microVM, so that external clients can resolve queries against the isolated environment.
- ✅ **Metric Definition:** Success = A user can specify a UDP port mapping (e.g., `--port 5353:53/udp`). The daemon successfully proxies thousands of UDP datagrams per second bidirectionally without packet corruption, and a standard `dig` query to the host port correctly resolves against the guest DNS server.
- **Functional Requirements:**
  - Update the configuration API and CLI parser to accept protocol specifications (e.g., `TCP`, `UDP`, or `BOTH`). If omitted, it must default to TCP for backward compatibility.
  - Implement a UDP proxy capability in the daemon that maintains a mapping of remote host endpoints to manage bidirectional datagram flow.
  - Ensure lifecycle integration: the UDP listeners must start when the VM boots and stop gracefully when the VM is shut down.

## 🚫 Out of Scope
- **SCTP or Raw IP Forwarding:** This phase focuses strictly on UDP. Other layer 4/3 protocols remain unsupported.
- **Dynamic Port Mapping:** Adding or removing port forwards while the VM is running is out of scope. Rules are applied statically at VM boot, identical to the current TCP implementation.
- **Load Balancing:** We are proxying traffic to a single guest IP and port. Distributing UDP traffic across multiple microVMs is out of scope.
