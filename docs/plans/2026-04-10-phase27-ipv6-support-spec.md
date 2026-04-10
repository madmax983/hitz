# 🔭 Vantage: Spec for IPv6 Support

## Problem Statement
Currently, Hitz's network configuration and data plane strictly mandate the use of IPv4 addresses. Users deploying microVMs into modern edge environments or data centers where IPv6 is standard (or mandated) cannot attach their VMs to the host network.

## The "So What?"
What business problem does this solve? Cloud-native adoption requires seamless interoperability with the broader network. As global IPv4 address pools exhaust and regional ISPs enforce IPv6-only architectures, lack of IPv6 support restricts Hitz from functioning in these modern networks. Adding IPv6 support ensures Hitz remains relevant, simplifies routing in large-scale cluster deployments, and removes the artificial limit of legacy addressing for our users.

## Gap Analysis
The configuration schema currently maps directly to IPv4 parsing logic and limits IPs to string representations assuming IPv4. Furthermore, the daemon's network code (such as the bridge data plane, L2↔L3 header translation, and port forwarding managers) hardcodes IPv4 structures and socket definitions. We lack a robust dual-stack or IPv6-native implementation across both the control and data planes.

## Acceptance Criteria
- 👤 **User Story:** As a Cloud Engineer, I want to assign an IPv6 address to the microVM and its host-side adapter, so that the VM can communicate over our IPv6-only data center network.
- 👤 **User Story:** As an Application Developer, I want to configure port forwarding to an IPv6 address inside the microVM, so that my IPv6-aware services are reachable from the host.
- ✅ **Metric Definition:** Success = A user can supply an IPv6 CIDR to `hitz vm create --net guest-ip=[IPv6/prefix]`. The daemon successfully allocates the network adapter with the corresponding IPv6 configurations, and a successful ICMPv6 echo request (`ping6`) confirms bidirectional connectivity. TCP/UDP port forwarding also proxies traffic correctly when bound to an IPv6 address.
- **Functional Requirements:**
  - Update the API to accept either IPv4, IPv6, or both natively for network configurations.
  - Refactor the port forwarding implementation to support IPv6 TCP/UDP listeners and dual-stack binding.
  - Update the host L2/L3 bridging logic to parse, inject, and route IPv6 packets alongside IPv4.

## 🚫 Out of Scope
- **DHCPv6 Server:** We will rely on static IP assignment via kernel command line or SLAAC, rather than implementing a full DHCPv6 server inside the daemon.
- **NAT64/DNS64 Translation:** Providing translation services for IPv4-only guests in an IPv6 environment is out of scope. Guests are expected to be IPv6-aware.