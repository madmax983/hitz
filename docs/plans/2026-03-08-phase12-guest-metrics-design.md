# Phase 12: Guest-Side Metrics — Design

## Goal

Expose full guest resource snapshots (CPU, memory, disk, network, top-N processes) to the host
via a virtio-vsock channel. Snapshots feed the Phase 11 OTel pipeline automatically and are
available on-demand via `hitz vm metrics <id>`.

## Scope

**In:** virtio-vsock device, guest agent (auto-injected into initramfs), periodic push to OTel,
on-demand pull via CLI, MessagePack wire format.

**Out:** Windows guest support, dynamic agent upgrade without restart, per-process network
attribution, guest-to-host file transfer over vsock (future phases).

## Architecture

```
Guest VM                                   Host (hitz-daemon)
────────────────────────────────────       ──────────────────────────────────────
hitz-guest-agent                           vsock_server.rs
  ├─ /proc/stat      → CpuMetrics           ├─ per-VM task
  ├─ /proc/meminfo   → MemoryMetrics        │    ├─ recv push snapshots
  ├─ /proc/diskstats → Vec<DiskMetrics>     │    │    └─► OTel gauges/counters
  ├─ /proc/net/dev   → Vec<NetMetrics>      │    └─ respond to pull requests
  ├─ /proc/*/stat    → Vec<ProcMetrics>     └─ GET /vms/{id}/metrics
  │                                              └─► on-demand vsock request
  ├─ push thread: every 5s → port 52355
  └─ pull thread: listen → respond on request
          │
          │  virtio-vsock (MMIO slot 2, IRQ 7)
          │  MessagePack-encoded MetricsSnapshot
          │  CID: guest=VM_INDEX+3, host=2
```

## New Crates / Files

| Path | Action |
|------|--------|
| `hitz-guest-agent/` | NEW crate — static musl binary |
| `hitz-devices/src/virtio/vsock.rs` | NEW — `VirtioVsockDevice` |
| `hitz-daemon/src/vsock_server.rs` | NEW — host-side multiplexer + OTel publisher |
| `hitz-api/src/lib.rs` | `MetricsSnapshot`, `GuestAgentMode`, `MetricsRequest` |
| `hitz-vmm/src/vm.rs` | wire vsock device into boot pipeline (MMIO slot 2) |
| `hitz-daemon/src/vm_manager.rs` | start/stop metrics task with VM lifecycle |
| `hitz-cli/src/main.rs` | `hitz vm metrics <id>` subcommand |
| `hitz-daemon/build.rs` | cross-compile agent, embed via `include_bytes!` |

## virtio-vsock Device

Device ID: `VIRTIO_ID_VSOCK = 19`
MMIO address: `0xD000_2000` (slot 2), IRQ 7

Three virtqueues:
- **RX (0)**: host → guest
- **TX (1)**: guest → host
- **Event (2)**: transport events (stub — return empty descriptors)

### Packet Header (44 bytes, `#[repr(C)]`)

```rust
pub struct VsockHdr {
    pub src_cid:   u64,
    pub dst_cid:   u64,
    pub src_port:  u32,
    pub dst_port:  u32,
    pub len:       u32,   // payload bytes following header
    pub r#type:    u16,   // always STREAM = 1
    pub op:        u16,   // VsockOp variant
    pub flags:     u32,
    pub buf_alloc: u32,   // receiver's total buffer size (flow control)
    pub fwd_cnt:   u32,   // bytes consumed so far (flow control)
}
```

### Operations

```rust
pub enum VsockOp {
    Request       = 1,  // guest initiates connection
    Response      = 2,  // host accepts
    Rst           = 3,  // reset connection
    Shutdown      = 4,  // half-close
    Rw            = 5,  // data
    CreditUpdate  = 6,  // flow control update
    CreditRequest = 7,  // request credit update from peer
}
```

### Flow Control

Credit-based: each side tracks `peer_buf_alloc` and `peer_fwd_cnt`. Available send window =
`peer_buf_alloc - peer_fwd_cnt - bytes_in_flight`. Sender blocks when window hits zero and
sends `CreditRequest`. Initial host buffer: 256 KiB per connection.

### Connection Lifecycle

```
Guest                    Host
  │── Request ──────────►│  (guest initiates)
  │◄─ Response ──────────│  (host accepts on port 52355)
  │── Rw ───────────────►│  (guest sends snapshot)
  │◄─ CreditUpdate ──────│  (host updates fwd_cnt)
  │── Shutdown ──────────►│  (connection close)
  │◄─ Rst ───────────────│
```

## Guest Agent

**Crate:** `hitz-guest-agent` (binary crate, `x86_64-unknown-linux-musl` target)

**Behaviour:** On startup, runs two concurrent loops:

1. **Push loop** — every `HITZ_METRICS_INTERVAL` seconds (default 5):
   - Read `/proc/stat`, `/proc/meminfo`, `/proc/diskstats`, `/proc/net/dev`, `/proc/*/stat`
   - Build `MetricsSnapshot`
   - Connect to CID 2 port 52355, write MessagePack frame, close

2. **Pull loop** — listens on CID=own, port 52355:
   - Accept connection from host (CID 2)
   - Read `MetricsRequest` packet
   - Respond with current `MetricsSnapshot` MessagePack frame

**Process top-N:** top 10 processes by CPU%, collected from `/proc/*/stat` diff between
two readings 500ms apart (same approach as `top`).

### Metrics Structs (shared via `hitz-api`)

```rust
pub struct MetricsSnapshot {
    pub timestamp_ms: u64,
    pub cpu:       CpuMetrics,
    pub memory:    MemoryMetrics,
    pub disks:     Vec<DiskMetrics>,
    pub networks:  Vec<NetMetrics>,
    pub processes: Vec<ProcMetrics>,
}

pub struct CpuMetrics {
    pub total_pct: f32,
    pub per_core:  Vec<f32>,
    pub load_avg:  [f32; 3],  // 1m, 5m, 15m
}

pub struct MemoryMetrics {
    pub total_bytes:    u64,
    pub used_bytes:     u64,
    pub free_bytes:     u64,
    pub buffers_bytes:  u64,
    pub cached_bytes:   u64,
    pub swap_total:     u64,
    pub swap_used:      u64,
}

pub struct DiskMetrics {
    pub name:          String,
    pub reads_total:   u64,
    pub writes_total:  u64,
    pub read_bytes:    u64,
    pub write_bytes:   u64,
}

pub struct NetMetrics {
    pub interface:   String,
    pub rx_bytes:    u64,
    pub tx_bytes:    u64,
    pub rx_packets:  u64,
    pub tx_packets:  u64,
    pub rx_errors:   u64,
    pub tx_errors:   u64,
}

pub struct ProcMetrics {
    pub pid:      u32,
    pub name:     String,
    pub cpu_pct:  f32,
    pub rss_bytes: u64,
    pub state:    char,
}

pub enum MetricsRequest {
    Snapshot,
}
```

## Initramfs Injection

At `start_vm`, when `guest_agent != Disabled`:

1. Build a newc-format cpio archive in memory containing:
   - `./sbin/hitz-agent` — the agent binary (mode `0o755`)
   - `./etc/init.d/S99hitz-agent` — start script (mode `0o755`):
     ```sh
     #!/bin/sh
     /sbin/hitz-agent &
     ```
2. Append this overlay to the user's initramfs bytes (cpio archives are concatenatable;
   kernel processes left-to-right, last entry wins on collision).

The embedded binary is included at compile time:
```rust
// hitz-daemon/src/lib.rs (behind `guest-agent` feature)
static AGENT_BYTES: &[u8] = include_bytes!(env!("HITZ_AGENT_BIN"));
```

`hitz-daemon/build.rs` cross-compiles `hitz-guest-agent` for `x86_64-unknown-linux-musl`
and sets `HITZ_AGENT_BIN` to the output path. The `guest-agent` Cargo feature gates this
so the daemon still compiles without the musl toolchain.

### VmConfig Addition

```rust
#[serde(default)]
pub guest_agent: GuestAgentMode,

pub enum GuestAgentMode {
    Auto,               // inject built-in agent (default)
    Custom(PathBuf),    // inject this binary instead
    Disabled,           // no injection
}
```

## Host-Side Integration

### `hitz-daemon/src/vsock_server.rs`

```rust
pub async fn run_metrics_task(
    vsock: Arc<VirtioVsockDevice>,
    vm_id: String,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            snap = vsock.recv_push_snapshot() => {
                publish_to_otel(&vm_id, &snap);
            }
            req = vsock.recv_pull_request() => {
                let snap = collect_snapshot_for(&vm_id);
                vsock.send_response(req.conn_id, snap).await;
            }
            _ = shutdown.changed() => break,
        }
    }
}
```

### OTel Metrics Published

| Metric | Type | Labels |
|--------|------|--------|
| `hitz.guest.cpu_usage` | Gauge | `vm.id`, `cpu` (`"total"` or core index) |
| `hitz.guest.memory_used_bytes` | Gauge | `vm.id` |
| `hitz.guest.memory_total_bytes` | Gauge | `vm.id` |
| `hitz.guest.disk_read_bytes_total` | Counter | `vm.id`, `disk` |
| `hitz.guest.disk_write_bytes_total` | Counter | `vm.id`, `disk` |
| `hitz.guest.net_rx_bytes_total` | Counter | `vm.id`, `interface` |
| `hitz.guest.net_tx_bytes_total` | Counter | `vm.id`, `interface` |

### `hitz vm metrics <id>` CLI

```
$ hitz vm metrics vm-abc123
CPU:     total=12.4%  cores=[8.1, 16.7, 9.2, 15.6]  load=0.42/0.38/0.31
Memory:  used=142 MiB / 256 MiB  (buffers=12M  cached=38M)
Disk:    vda  reads=1204  writes=892  read=48M  write=21M
Net:     eth0  rx=2.1M  tx=892K  rx_pkt=1842  tx_pkt=634
Procs:   top 5 by CPU
           PID  NAME            CPU%   RSS
          1042  nginx            8.2%  18M
           891  redis-server     3.1%  32M
           ...
```

Daemon route: `GET /vms/{id}/metrics` — sends `MetricsRequest::Snapshot` over vsock,
awaits response, returns `MetricsSnapshot` as JSON.

## MMIO Layout (updated)

```
0xD000_0000  virtio-blk   (slot 0, IRQ 5)
0xD000_1000  virtio-net   (slot 1, IRQ 6)
0xD000_2000  virtio-vsock (slot 2, IRQ 7)  ← Phase 12
```

## Shutdown

`VirtioVsockDevice` stored in `VmEntry` alongside `_port_fwd`. Dropped when VM stops —
the guest agent detects the vsock connection reset and exits cleanly.

## Build Prerequisites

Cross-compilation from Windows requires the musl target and a Linux cross-linker:
```
rustup target add x86_64-unknown-linux-musl
# cross-linker: cargo-zigbuild (zig cc) or cargo-cross
cargo install cargo-zigbuild
```

The `guest-agent` feature is opt-in; without it the daemon compiles normally but
`GuestAgentMode::Auto` falls back to `Disabled` with a warning.

## Testing

**Unit (no WHP):**
- `VsockHdr` encode/decode roundtrip
- Flow control: credit window accounting
- cpio overlay: verify file entries parseable by a reference cpio reader
- `MetricsSnapshot` MessagePack roundtrip
- `/proc` parser unit tests with fixture strings

**Integration (`#[ignore]`, WHP + musl toolchain):**
- `phase12_vsock_metrics`: boot Linux guest with agent injected, connect from host,
  assert `MetricsSnapshot` received with `cpu.total_pct >= 0.0` and `memory.total_bytes > 0`
