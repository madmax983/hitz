# 🔭 Vantage: Spec for Network Bandwidth QoS (Rate Limiting)

## Problem Statement
In multi-tenant or resource-constrained environments, a single microVM can saturate the host's network interface by transmitting or receiving excessive data. Currently, Hitz provides raw access to the host's network via `hitz-net` and WinTun, meaning one "noisy neighbor" VM can degrade network performance for all other VMs on the same machine.

## The "So What?"
Users operating container-like orchestration on top of Hitz need predictability. Without network QoS (Quality of Service) rate limiting, it is impossible to sell tiered network performance or guarantee SLAs for tenant applications. By implementing rate limiting, we transform raw compute into a manageable, multi-tenant product.

## Gap Analysis
Production-grade hypervisors (like Firecracker) and container runtimes (like Docker/Kubernetes via CNI plugins) allow administrators to configure bandwidth limits (token bucket rate limiters) on network interfaces. Hitz currently lacks this essential capability in its network architecture.

## Goal
Provide a configurable token-bucket rate limiter for the `hitz-net` virtio-net implementation to restrict inbound (RX) and outbound (TX) bandwidth and operations per second (OPS) on a per-VM basis.

## Scope

**In Scope:**
- Adding `rx_bytes_rate`, `tx_bytes_rate`, `rx_ops_rate`, and `tx_ops_rate` configuration options to the VM network configuration.
- Implementing a token-bucket rate limiting mechanism within the `hitz-net` virtio queue processing loop.
- Modifying `VmConfig` and `CreateVmRequest` to accept QoS parameters.

**Out of Scope:**
- Dynamic updates to QoS limits on a running VM (this can be a phase 2 feature; currently, limits are applied at boot).
- CPU or Storage QoS (handled in separate specs).
- Complex traffic shaping algorithms (e.g., HTB) beyond simple token bucket limiting.

## Acceptance Criteria
- **User Story:** As a Cloud Provider, I want to limit the network bandwidth of a VM to 100 Mbps, so that a single customer cannot monopolize the host's network adapter.
- **Metric:** The network throughput measured by a guest utility (e.g., `iperf3`) must not exceed the configured limit + 5% burst allowance.
- **Functionality:**
  - The CLI and API must accept rate limiting parameters when creating a network interface.
  - The `hitz-net` virtio-net implementation must delay or drop packets if the token bucket is exhausted, respecting the configured bandwidth and ops limits.

## Proposed API Additions

### `VmConfig` Updates
```json
{
  "network": {
    "backend": "wintun",
    "rate_limit": {
      "bandwidth": {
        "size": 12500000, // bytes per second (100 Mbps)
        "refill_time": 100 // milliseconds
      },
      "ops": {
        "size": 10000, // operations per second
        "refill_time": 100 // milliseconds
      }
    }
  }
}
```

### CLI Updates
```bash
hitz vm create \
  --kernel vmlinux \
  --disk rootfs.ext4 \
  --net-bw-limit 12500000 \
  --net-ops-limit 10000
```
