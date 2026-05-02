# 🔭 Vantage: Spec for Isolated Virtual Networks (VPC)

## Problem Statement
Currently, Hitz connects microVMs to the host network via NAT or port forwarding. While this enables host-to-guest and guest-to-internet communication, there is no native way to create an isolated, private network (Virtual Switch) where multiple microVMs can communicate securely with each other without routing traffic through the host's external stack.

## The "So What?"
What business problem does this solve? Modern applications are distributed and multi-tier (e.g., a frontend web server and a backend database). Customers need to deploy these architectures securely, ensuring backend databases are completely unreachable from the outside world. Without isolated virtual networks, developers must rely on complex, error-prone host firewall rules to simulate isolation. By providing native VPC capabilities, Hitz becomes capable of orchestrating complex, secure multi-VM environments, unlocking enterprise deployments and competing with traditional cloud-provider network models.

## Gap Analysis
Standard hypervisors and container platforms (like Docker Compose or KVM/libvirt) support custom bridge networks that isolate groups of instances. Hitz currently lacks a software-defined switch implementation to route packets directly between virtio-net devices of different VMs. We are missing the concept of a "Network" resource in our API.

## Acceptance Criteria
- 👤 **User Story:** As a Cloud Architect, I want to create an isolated virtual network and attach multiple microVMs to it, so that my multi-tier applications can communicate securely without exposing internal traffic to the host network.
- ✅ **Metric Definition:** Success = A user can create a private network via `hitz network create my-vpc`, attach two microVMs to it, and successfully achieve >1 Gbps throughput between the two VMs using `iperf3`, while both VMs remain completely unreachable from the host's external network adapters.
- **Functional Requirements:**
  - Introduce a new REST API and CLI commands to manage virtual networks (`hitz network create/ls/rm`).
  - Implement a software-defined switch within the daemon that bridges virtio-net interfaces belonging to the same network.
  - Ensure broadcast and multicast traffic works correctly within the isolated network to support auto-discovery protocols.

## 🚫 Out of Scope
- **Cross-Host Overlay Networks:** This phase strictly targets isolated networks on a single physical host. Multi-node VXLAN or overlay networking is out of scope.
- **Advanced Network Policies (Firewalls):** Implementing guest-to-guest firewall rules (e.g., allowing port 80 but blocking port 22 between VMs) is deferred. The network operates as a flat Layer 2 switch.
