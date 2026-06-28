# Hitz: A Lightweight Micro-VM Infrastructure

## Abstract
Hitz is a high-performance, Rust-based Virtual Machine Monitor (VMM) and ecosystem designed for running extremely lightweight micro-VMs. It prioritizes fast boot times, minimal memory footprint, and deep telemetry integration.

## The Hero's Journey
Hitz enables developers and operations to define, launch, and monitor VMs declaratively.

### Core Architecture
- **`hitz-vmm`**: The core hypervisor abstraction.
- **`hitz-api`**: The REST API and telemetry structures used to control and interrogate VMs. Includes tools for `CarbonEstimator`, `RightSizer`, and `WorkloadClassifier`.
- **`hitz-daemon`**: The background service managing VM lifecycles and state.
- **`hitz-cli`**: The user-friendly command-line interface.

### Example: Checking VM Health
```rust
use hitz_api::{HealthCheck, HealthStatus, MetricsSnapshot, CpuMetrics, MemoryMetrics};

let metrics = MetricsSnapshot {
    timestamp_ms: 1_700_000_000_000,
    cpu: CpuMetrics { total_pct: 42.5, per_core: vec![40.0, 45.0], load_avg: [1.2, 0.8, 0.5] },
    memory: MemoryMetrics {
        total_bytes: 1024 * 1024 * 1024,
        used_bytes: 256 * 1024 * 1024,
        free_bytes: 768 * 1024 * 1024,
        buffers_bytes: 0,
        cached_bytes: 0,
        swap_total: 0,
        swap_used: 0,
    },
    disks: vec![],
    networks: vec![],
    processes: vec![],
};

let health = metrics.assess_health();
```

## The Fine Print
- Designed primarily for Linux guests.
- Supports `virtio-net`, `virtio-blk`, and `virtio-vsock` devices.
- Built-in metrics aggregation requires the `hitz-guest-agent`.
