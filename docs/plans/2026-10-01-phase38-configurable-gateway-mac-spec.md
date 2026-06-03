# 🔭 Vantage: Spec for Configurable Gateway MAC

## Problem Statement
Currently, the Hitz hypervisor hardcodes the Virtual Gateway MAC address to `AA:BB:CC:00:00:01` during the network initialization phase of the microVM boot process. This rigid configuration prevents network administrators from seamlessly integrating Hitz microVMs into strict Layer 2 environments, custom overlay networks, or advanced virtual switches that employ MAC whitelisting and spoofing protections.

## The "So What?"
What business problem does this solve? Network interoperability and security compliance in enterprise environments. When deploying high-density microVM architectures across sophisticated datacenter networks, hardcoded MAC addresses cause collision domains or trigger security alarms resulting in dropped packets. By allowing the Gateway MAC to be configurable via our API, we unlock deployment in enterprise network topologies that require explicit MAC management, increasing the platform's viability for high-security infrastructure customers.

## Gap Analysis
- **Current State:** The `NetConfig` struct in `crates/hitz-api/src/config.rs` supports configuring the `host_ip`, `guest_ip`, and the guest's `mac` address. However, the host-side gateway MAC is explicitly hardcoded in `crates/hitz-vmm/src/vm.rs` as `let gateway_mac: [u8; 6] = [0xAA, 0xBB, 0xCC, 0x00, 0x00, 0x01];`.
- **Market Standard:** Enterprise virtualization platforms allow defining exact MAC addresses for both endpoints of a virtual interface bridge, avoiding any hidden or assumed values on the data plane.

## Acceptance Criteria
- 👤 **User Story:** As a Network Administrator, I want to configure the Gateway MAC address of my microVM's virtual network interface, so that it can integrate seamlessly with strict Layer 2 security policies or existing overlay networks without encountering MAC collisions.
- ✅ **Metric Definition:** Success = A user specifies a custom `gateway_mac` in the `NetConfig` payload during VM creation. The daemon boots the VM and routes traffic via the newly specified MAC address, resulting in zero packet drops and verifiable ARP responses matching the user-provided address in under 1 second.
- **Functional Requirements:**
  - Add an optional `gateway_mac: Option<String>` field to the `NetConfig` structure in `hitz-api`.
  - Update the parsing logic to validate the string as a proper 6-byte MAC address.
  - Modify `crates/hitz-vmm/src/vm.rs` to consume this configured value or fallback to the current `AA:BB:CC:00:00:01` default if none is provided.

## 🚫 Out of Scope
- **Dynamic MAC Address Rotation:** The scope covers defining the Gateway MAC statically at boot time. Dynamically rotating the MAC address while the microVM is actively running is explicitly out of scope.
- **Support for non-Ethernet L2 Protocols:** We assume an underlying 802.3 Ethernet frame structure. InfiniBand or other data link layer integrations remain unsupported.
