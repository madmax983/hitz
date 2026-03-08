# Phase 12: Guest-Side Metrics — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expose full guest resource snapshots (CPU, memory, disk, network, top-10 processes) to the host via a new virtio-vsock device; push snapshots to OTel automatically and serve them on-demand via `hitz vm metrics <id>`.

**Architecture:** A new `VirtioVsockDevice` (device ID 19, MMIO slot 2) uses crossbeam channels to bridge the sync vCPU run-loop with the async daemon. A static musl-compiled guest agent reads `/proc/*` files, serializes with MessagePack, and sends snapshots over AF_VSOCK. The daemon publishes metrics to OTel and exposes a pull endpoint.

**Tech Stack:** Rust 2024, `rmp-serde` (MessagePack), `vsock` crate (guest side), crossbeam channels (sync↔async bridge), `cargo-zigbuild` (musl cross-compile), newc cpio format (agent injection).

---

## Prerequisites (verify before starting)

```bash
# Add musl target
rustup target add x86_64-unknown-linux-musl

# Install zig-based cross-linker (handles musl libc on Windows)
cargo install cargo-zigbuild

# Verify
cargo zigbuild --version
```

If `cargo-zigbuild` is unavailable, Tasks 6–7 can use a pre-built binary committed to `assets/hitz-agent` instead. The plan notes this fallback where relevant.

---

## Task dependency graph

```
Task 1 (API types)
  ├──► Task 2 (VsockHdr + protocol)
  │      └──► Task 3 (VirtioVsockDevice)
  │             ├──► Task 4 (wire into vm.rs)
  │             └──► Task 8 (host vsock_server)
  ├──► Task 5 (cpio builder)         ┐
  └──► Task 6 (guest agent binary)   ├─ PARALLEL after Task 1
         └──► Task 7 (embed + inject) ┘
Tasks 4 + 7 + 8 ──► Task 9 (VmEntry wiring)
Task 9 ──► Task 10 (CLI + route)
Task 10 ──► Task 11 (integration test)
```

---

## Task 1: API types in `hitz-api`

**Files:**
- Modify: `crates/hitz-api/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — add `rmp-serde`)

### Step 1.1 — Add `rmp-serde` to workspace

In `Cargo.toml` `[workspace.dependencies]`:
```toml
rmp-serde = "1"
```

### Step 1.2 — Write failing tests (RED)

Add to `crates/hitz-api/src/lib.rs` in a `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_msgpack_roundtrip() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1_700_000_000_000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 156 * 1024 * 1024,
                buffers_bytes: 10 * 1024 * 1024,
                cached_bytes: 30 * 1024 * 1024,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let encoded = rmp_serde::to_vec(&snap).expect("encode");
        let decoded: MetricsSnapshot = rmp_serde::from_slice(&encoded).expect("decode");
        assert!((decoded.cpu.total_pct - 12.5).abs() < f32::EPSILON);
        assert_eq!(decoded.memory.total_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn guest_agent_mode_default_is_auto() {
        let mode: GuestAgentMode = GuestAgentMode::default();
        assert!(matches!(mode, GuestAgentMode::Auto));
    }

    #[test]
    fn vm_config_guest_agent_serde_default() {
        // Old configs without guest_agent field should deserialize as Auto.
        let json = r#"{"kernel_path":"/k","ram_mib":256,"cpus":1}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert!(matches!(cfg.guest_agent, GuestAgentMode::Auto));
    }

    #[test]
    fn vm_config_guest_cid_serde_default() {
        let json = r#"{"kernel_path":"/k","ram_mib":256,"cpus":1}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.guest_cid, DEFAULT_GUEST_CID);
    }
}
```

Run:
```bash
cargo test -p hitz-api 2>&1 | head -20
# Expected: compile errors — MetricsSnapshot, GuestAgentMode not found
```

### Step 1.3 — Implement

Add to `crates/hitz-api/src/lib.rs`:

```rust
use std::path::PathBuf;

/// Default guest CID for virtio-vsock (host=2, first guest=3).
pub const DEFAULT_GUEST_CID: u32 = 3;

/// Metrics port on which the guest agent listens and the host connects.
pub const VSOCK_METRICS_PORT: u32 = 52355;

// ── Guest agent mode ─────────────────────────────────────────────────────────

/// Controls whether and which guest metrics agent is injected into the initramfs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum GuestAgentMode {
    /// Automatically inject the built-in agent (default).
    #[default]
    Auto,
    /// Inject a user-supplied agent binary instead of the built-in one.
    Custom(PathBuf),
    /// Do not inject any agent.
    Disabled,
}

// ── Metrics wire protocol ────────────────────────────────────────────────────

/// On-demand metrics request sent from host to guest over vsock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsRequest {
    /// Request a full resource snapshot.
    Snapshot,
}

// ── Metrics snapshot ─────────────────────────────────────────────────────────

/// Full guest resource snapshot, serialized with MessagePack over vsock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disks: Vec<DiskMetrics>,
    pub networks: Vec<NetMetrics>,
    /// Top processes by CPU usage (up to 10).
    pub processes: Vec<ProcMetrics>,
}

/// CPU utilisation metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Overall CPU utilisation percentage (0.0–100.0).
    pub total_pct: f32,
    /// Per-core utilisation percentages.
    pub per_core: Vec<f32>,
    /// Load averages: 1-minute, 5-minute, 15-minute.
    pub load_avg: [f32; 3],
}

/// Memory utilisation metrics (all in bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// Per-disk I/O metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    pub name: String,
    pub reads_total: u64,
    pub writes_total: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// Per-network-interface metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetMetrics {
    pub interface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

/// Per-process metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcMetrics {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub rss_bytes: u64,
    pub state: char,
}
```

Also add `rmp-serde.workspace = true` to `crates/hitz-api/Cargo.toml` under `[dependencies]`.

Add two new fields to `VmConfig`:
```rust
/// Virtio-vsock guest CID. Must be unique per running VM. Default: 3.
#[serde(default = "default_guest_cid")]
pub guest_cid: u32,

/// Guest metrics agent injection mode.
#[serde(default)]
pub guest_agent: GuestAgentMode,
```

Add helper:
```rust
const fn default_guest_cid() -> u32 {
    DEFAULT_GUEST_CID
}
```

### Step 1.4 — Run GREEN

```bash
cargo test -p hitz-api -- --nocapture 2>&1 | tail -10
# Expected: all tests pass including the 4 new ones

cargo clippy -p hitz-api -- -D warnings
cargo fmt --check
```

### Step 1.5 — Commit

```bash
git add Cargo.toml crates/hitz-api/src/lib.rs crates/hitz-api/Cargo.toml
git commit -m "feat(api): MetricsSnapshot types, GuestAgentMode, guest_cid for Phase 12"
```

---

## Task 2: VsockHdr + protocol types

**Files:**
- Create: `crates/hitz-devices/src/virtio/vsock.rs`
- Modify: `crates/hitz-devices/src/virtio/mod.rs` (add `pub mod vsock`)

**Depends on:** Task 1

### Step 2.1 — Write failing test (RED)

Create `crates/hitz-devices/src/virtio/vsock.rs` with just the test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsock_hdr_roundtrip() {
        let hdr = VsockHdr {
            src_cid: 3,
            dst_cid: 2,
            src_port: 12345,
            dst_port: 52355,
            len: 42,
            r#type: VSOCK_TYPE_STREAM,
            op: VsockOp::Rw as u16,
            flags: 0,
            buf_alloc: 262_144,
            fwd_cnt: 0,
        };
        let bytes = hdr.to_bytes();
        assert_eq!(bytes.len(), VSOCK_HDR_SIZE);
        let parsed = VsockHdr::from_bytes(&bytes).expect("parse");
        assert_eq!(parsed.src_cid, 3);
        assert_eq!(parsed.dst_cid, 2);
        assert_eq!(parsed.len, 42);
        assert_eq!(parsed.op, VsockOp::Rw as u16);
    }

    #[test]
    fn vsock_op_known_values() {
        assert_eq!(VsockOp::Request as u16, 1);
        assert_eq!(VsockOp::Response as u16, 2);
        assert_eq!(VsockOp::Rst as u16, 3);
        assert_eq!(VsockOp::Shutdown as u16, 4);
        assert_eq!(VsockOp::Rw as u16, 5);
        assert_eq!(VsockOp::CreditUpdate as u16, 6);
        assert_eq!(VsockOp::CreditRequest as u16, 7);
    }
}
```

Run:
```bash
cargo test -p hitz-devices vsock 2>&1 | head -10
# Expected: compile error — VsockHdr not found
```

### Step 2.2 — Implement

Full content of `crates/hitz-devices/src/virtio/vsock.rs`:

```rust
//! Virtio-vsock packet header and protocol constants.
//!
//! Reference: virtio spec 1.2, section 5.10.
//! Linux kernel: `include/uapi/linux/virtio_vsock.h`

/// Size of the virtio-vsock packet header in bytes (44 bytes).
pub const VSOCK_HDR_SIZE: usize = 44;

/// Stream socket type (the only type we implement).
pub const VSOCK_TYPE_STREAM: u16 = 1;

/// Host CID (as defined by the virtio-vsock spec).
pub const VMADDR_CID_HOST: u64 = 2;

/// Initial receive buffer size advertised per connection (256 KiB).
pub const VSOCK_BUF_ALLOC: u32 = 256 * 1024;

/// Virtio-vsock packet operations.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsockOp {
    /// Guest requests a connection.
    Request      = 1,
    /// Host accepts a connection.
    Response     = 2,
    /// Reset / reject a connection.
    Rst          = 3,
    /// Shutdown one or both directions.
    Shutdown     = 4,
    /// Data transfer.
    Rw           = 5,
    /// Flow-control credit update.
    CreditUpdate = 6,
    /// Request a credit update from the peer.
    CreditRequest = 7,
}

impl VsockOp {
    /// Parse a raw u16 into a `VsockOp`. Returns `None` for unknown values.
    #[must_use]
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Self::Request),
            2 => Some(Self::Response),
            3 => Some(Self::Rst),
            4 => Some(Self::Shutdown),
            5 => Some(Self::Rw),
            6 => Some(Self::CreditUpdate),
            7 => Some(Self::CreditRequest),
            _ => None,
        }
    }
}

/// Virtio-vsock packet header (44 bytes, little-endian).
///
/// Follows `struct virtio_vsock_hdr` from the Linux kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VsockHdr {
    pub src_cid:   u64,
    pub dst_cid:   u64,
    pub src_port:  u32,
    pub dst_port:  u32,
    /// Payload length in bytes (not including this header).
    pub len:       u32,
    /// Always `VSOCK_TYPE_STREAM`.
    pub r#type:    u16,
    /// Operation (see [`VsockOp`]).
    pub op:        u16,
    /// Flags (used for SHUTDOWN direction bits).
    pub flags:     u32,
    /// Receiver's total buffer allocation (flow control).
    pub buf_alloc: u32,
    /// Bytes consumed by receiver so far (flow control).
    pub fwd_cnt:   u32,
}

impl VsockHdr {
    /// Serialize header to 44 little-endian bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; VSOCK_HDR_SIZE] {
        let mut b = [0u8; VSOCK_HDR_SIZE];
        b[0..8].copy_from_slice(&self.src_cid.to_le_bytes());
        b[8..16].copy_from_slice(&self.dst_cid.to_le_bytes());
        b[16..20].copy_from_slice(&self.src_port.to_le_bytes());
        b[20..24].copy_from_slice(&self.dst_port.to_le_bytes());
        b[24..28].copy_from_slice(&self.len.to_le_bytes());
        b[28..30].copy_from_slice(&self.r#type.to_le_bytes());
        b[30..32].copy_from_slice(&self.op.to_le_bytes());
        b[32..36].copy_from_slice(&self.flags.to_le_bytes());
        b[36..40].copy_from_slice(&self.buf_alloc.to_le_bytes());
        b[40..44].copy_from_slice(&self.fwd_cnt.to_le_bytes());
        b
    }

    /// Deserialize header from 44 little-endian bytes.
    ///
    /// # Errors
    /// Returns `None` if `bytes.len() < 44`.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < VSOCK_HDR_SIZE {
            return None;
        }
        Some(Self {
            src_cid:   u64::from_le_bytes(bytes[0..8].try_into().ok()?),
            dst_cid:   u64::from_le_bytes(bytes[8..16].try_into().ok()?),
            src_port:  u32::from_le_bytes(bytes[16..20].try_into().ok()?),
            dst_port:  u32::from_le_bytes(bytes[20..24].try_into().ok()?),
            len:       u32::from_le_bytes(bytes[24..28].try_into().ok()?),
            r#type:    u16::from_le_bytes(bytes[28..30].try_into().ok()?),
            op:        u16::from_le_bytes(bytes[30..32].try_into().ok()?),
            flags:     u32::from_le_bytes(bytes[32..36].try_into().ok()?),
            buf_alloc: u32::from_le_bytes(bytes[36..40].try_into().ok()?),
            fwd_cnt:   u32::from_le_bytes(bytes[40..44].try_into().ok()?),
        })
    }

    /// Build a response header (swaps src/dst, sets the given op).
    #[must_use]
    pub fn make_response(&self, op: VsockOp, payload_len: u32) -> Self {
        Self {
            src_cid:   self.dst_cid,
            dst_cid:   self.src_cid,
            src_port:  self.dst_port,
            dst_port:  self.src_port,
            len:       payload_len,
            r#type:    VSOCK_TYPE_STREAM,
            op:        op as u16,
            flags:     0,
            buf_alloc: VSOCK_BUF_ALLOC,
            fwd_cnt:   0,
        }
    }
}
```

Add `pub mod vsock;` to `crates/hitz-devices/src/virtio/mod.rs`.

### Step 2.3 — Run GREEN

```bash
cargo test -p hitz-devices vsock -- --nocapture
# Expected: test virtio::vsock::tests::vsock_hdr_roundtrip ... ok
#           test virtio::vsock::tests::vsock_op_known_values ... ok

cargo clippy -p hitz-devices -- -D warnings
cargo fmt --check
```

### Step 2.4 — Commit

```bash
git add crates/hitz-devices/src/virtio/vsock.rs \
        crates/hitz-devices/src/virtio/mod.rs
git commit -m "feat(vsock): VsockHdr and protocol types"
```

---

## Task 3: `VirtioVsockDevice`

**Files:**
- Modify: `crates/hitz-devices/src/virtio/vsock.rs`
- Modify: `crates/hitz-devices/Cargo.toml` (add `crossbeam-channel.workspace = true` if not already there; check first)
- Modify: `crates/hitz-devices/src/lib.rs` (re-export)

**Depends on:** Task 2

### Step 3.1 — Write failing tests (RED)

Add to the `#[cfg(test)]` block in `vsock.rs`:

```rust
#[test]
fn vsock_device_construction() {
    let (device, _rx_recv, _tx_send) = VirtioVsockDevice::new(3);
    assert_eq!(device.device_id(), 19);
    assert_eq!(device.queue_count(), 3);
}

#[test]
fn vsock_config_returns_guest_cid() {
    let (device, _rx_recv, _tx_send) = VirtioVsockDevice::new(42);
    let mut buf = [0u8; 8];
    device.read_config(0, &mut buf);
    let cid = u64::from_le_bytes(buf);
    assert_eq!(cid, 42);
}
```

Run:
```bash
cargo test -p hitz-devices vsock 2>&1 | head -10
# Expected: compile error — VirtioVsockDevice not found
```

### Step 3.2 — Implement

Add to `crates/hitz-devices/src/virtio/vsock.rs`, after the protocol types:

```rust
use std::collections::VecDeque;

use crossbeam_channel::{Receiver, Sender};
use hitz_hal::GuestMemAccess;

use crate::virtio::mmio_transport::VirtioBackend;
use crate::virtio::queue::VirtQueue;

/// Virtio-vsock device (device ID 19).
///
/// Bridges the guest virtio driver with host-side socket handling.
/// Packets from the guest arrive on the TX queue (queue 1); packets
/// to the guest are injected via the RX queue (queue 0).
///
/// The event queue (queue 2) is stubbed — descriptors are immediately
/// returned unused, which satisfies the Linux driver.
///
/// Use [`VirtioVsockDevice::new`] to construct; the returned channel
/// endpoints connect to the host-side async runtime.
pub struct VirtioVsockDevice {
    /// Guest CID assigned to this VM.
    guest_cid: u64,
    /// Send guest→host packets (TX queue contents) to the host runtime.
    tx_sender: Sender<(VsockHdr, Vec<u8>)>,
    /// Receive host→guest packets (to inject into RX queue).
    rx_receiver: Receiver<(VsockHdr, Vec<u8>)>,
    /// Pending packets waiting to be written into the RX virtqueue.
    rx_pending: VecDeque<(VsockHdr, Vec<u8>)>,
}

impl VirtioVsockDevice {
    /// Create a new vsock device for the given guest CID.
    ///
    /// Returns:
    /// - The device (for the MMIO transport in the vCPU thread)
    /// - A `Receiver` for the host runtime to read TX packets from the guest
    /// - A `Sender` for the host runtime to inject RX packets into the guest
    #[must_use]
    pub fn new(guest_cid: u32) -> (
        Self,
        Receiver<(VsockHdr, Vec<u8>)>,
        Sender<(VsockHdr, Vec<u8>)>,
    ) {
        let (tx_sender, tx_receiver) = crossbeam_channel::unbounded();
        let (rx_sender, rx_receiver) = crossbeam_channel::unbounded();
        let device = Self {
            guest_cid: u64::from(guest_cid),
            tx_sender,
            rx_receiver,
            rx_pending: VecDeque::new(),
        };
        (device, tx_receiver, rx_sender)
    }

    /// Process the TX virtqueue: read all pending guest→host packets and
    /// forward them to the host via `tx_sender`.
    fn process_tx(&self, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        while let Some(mut chain) = queue.pop_chain(mem) {
            let head = chain.head_index();
            let mut raw = Vec::new();

            while let Some(desc) = chain.next_descriptor(mem) {
                if desc.is_device_writable {
                    break; // TX descriptors are all device-readable
                }
                let mut buf = vec![0u8; desc.len as usize];
                if mem.read_guest(desc.addr, &mut buf).is_ok() {
                    raw.extend_from_slice(&buf);
                }
            }

            if raw.len() >= VSOCK_HDR_SIZE {
                if let Some(hdr) = VsockHdr::from_bytes(&raw) {
                    let payload = raw[VSOCK_HDR_SIZE..][..hdr.len as usize].to_vec();
                    // Non-blocking send; if channel is closed, drop packet.
                    let _ = self.tx_sender.try_send((hdr, payload));
                }
            }

            queue.push_used(head, 0, mem);
        }
    }

    /// Stub the event virtqueue: return all pending descriptors unused.
    /// The Linux vsock driver expects descriptors it posted to come back.
    fn process_event(&self, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        while let Some(chain) = queue.pop_chain(mem) {
            queue.push_used(chain.head_index(), 0, mem);
        }
    }
}

impl VirtioBackend for VirtioVsockDevice {
    fn device_id(&self) -> u32 {
        19 // VIRTIO_ID_VSOCK
    }

    fn device_features(&self) -> u64 {
        0 // No optional features
    }

    fn queue_count(&self) -> usize {
        3 // RX=0, TX=1, Event=2
    }

    fn process_queue(&mut self, queue_idx: u16, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        match queue_idx {
            0 => { /* RX: guest posts receive buffers, no action needed */ }
            1 => self.process_tx(queue, mem),
            2 => self.process_event(queue, mem),
            _ => tracing::warn!(queue_idx, "vsock: unexpected queue notify"),
        }
    }

    fn poll_rx(&mut self, rx_queue: &mut VirtQueue, mem: &dyn GuestMemAccess) -> bool {
        // Drain channel into local pending queue.
        while let Ok(packet) = self.rx_receiver.try_recv() {
            self.rx_pending.push_back(packet);
        }

        if self.rx_pending.is_empty() {
            return false;
        }

        let mut injected = false;

        while let Some((hdr, payload)) = self.rx_pending.pop_front() {
            // Try to inject one packet into the RX virtqueue.
            if let Some(mut chain) = rx_queue.pop_chain(mem) {
                let head = chain.head_index();
                let hdr_bytes = hdr.to_bytes();
                let total = hdr_bytes.len() + payload.len();

                // Find a writable descriptor to write into.
                if let Some(desc) = chain.next_descriptor(mem) {
                    if desc.is_device_writable {
                        let _ = mem.write_guest(desc.addr, &hdr_bytes);
                        if !payload.is_empty() {
                            let payload_addr = desc.addr + hdr_bytes.len() as u64;
                            let _ = mem.write_guest(payload_addr, &payload);
                        }
                        rx_queue.push_used(head, total as u32, mem);
                        injected = true;
                        continue;
                    }
                }
                // Couldn't write — push packet back for next poll.
                self.rx_pending.push_front((hdr, payload));
                rx_queue.push_used(head, 0, mem);
                break;
            } else {
                // No RX buffers available — push packet back.
                self.rx_pending.push_front((hdr, payload));
                break;
            }
        }

        injected
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space: guest_cid at offset 0 (8 bytes, little-endian).
        let cid_bytes = self.guest_cid.to_le_bytes();
        let src_start = offset as usize;
        let src_end = (src_start + data.len()).min(cid_bytes.len());
        if src_start < cid_bytes.len() {
            let len = src_end - src_start;
            data[..len].copy_from_slice(&cid_bytes[src_start..src_end]);
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // Guest cannot change the CID.
    }
}
```

Add to `crates/hitz-devices/src/lib.rs`:
```rust
pub use virtio::vsock::VirtioVsockDevice;
```

### Step 3.3 — Check `crossbeam-channel` dep

```bash
grep -n "crossbeam" crates/hitz-devices/Cargo.toml
```

If not present, add to `crates/hitz-devices/Cargo.toml`:
```toml
crossbeam-channel.workspace = true
```

### Step 3.4 — Run GREEN

```bash
cargo test -p hitz-devices vsock -- --nocapture
# Expected: all 4 vsock tests pass

cargo clippy -p hitz-devices -- -D warnings
cargo fmt --check
```

**Clippy note on `too_many_lines`**: If `poll_rx` triggers it, add `#[allow(clippy::too_many_lines)]`.

### Step 3.5 — Commit

```bash
git add crates/hitz-devices/src/virtio/vsock.rs \
        crates/hitz-devices/src/lib.rs \
        crates/hitz-devices/Cargo.toml
git commit -m "feat(vsock): VirtioVsockDevice implementing VirtioBackend"
```

---

## Task 4: Wire vsock into boot pipeline

**Files:**
- Modify: `crates/hitz-vmm/src/vm.rs`

**Depends on:** Task 3

### Step 4.1 — Write failing test (RED)

The integration tests in `hitz-whp` will cover the full path. For now, add a unit test in `vm.rs` that exercises the MMIO constant layout:

Add to `crates/hitz-vmm/src/vm.rs` in the `#[cfg(test)]` block (check if one exists; if not, create it):

```rust
#[test]
fn vsock_mmio_slot_does_not_overlap_net() {
    // Slot 0 = blk (0xD000_0000), slot 1 = net (0xD000_1000),
    // slot 2 = vsock (0xD000_2000). Verify no overlap.
    let blk  = VIRTIO_MMIO_BASE;
    let net  = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
    let vsock = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
    assert_ne!(blk, net);
    assert_ne!(net, vsock);
    assert_eq!(VIRTIO_IRQ_VSOCK, 7);
}
```

Run:
```bash
cargo test -p hitz-vmm vsock_mmio -- --nocapture
# Expected: compile error — VIRTIO_IRQ_VSOCK not defined
```

### Step 4.2 — Implement

In `crates/hitz-vmm/src/vm.rs`, add the constant alongside the existing MMIO constants:

```rust
/// IRQ vector for the virtio-vsock device.
const VIRTIO_IRQ_VSOCK: u8 = 7;
```

Add the vsock import:
```rust
use hitz_devices::virtio::vsock::VirtioVsockDevice;
```

In `boot_and_run`, after step 13b (virtio-net setup) add step 13c:

```rust
// ── 13c. Optional virtio-vsock (guest metrics agent) ──
//
// The `VsockIoHandle` holds the channel endpoints so the host-side
// async runtime can communicate with the device. It must stay alive
// until after the run loop exits.
let vsock_io_handle: Option<crate::vsock_io::VsockIoHandle> =
    if config.guest_agent != hitz_api::GuestAgentMode::Disabled {
        let (vsock_dev, tx_rx, rx_tx) =
            VirtioVsockDevice::new(config.guest_cid);
        let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
        let mem: Arc<dyn GuestMemAccess> = guest_mem_arc.clone();
        let transport = VirtioMmioTransport::new(vsock_dev, mem, VIRTIO_IRQ_VSOCK);
        mmio_bus.register(vsock_base, VIRTIO_MMIO_SIZE, Box::new(transport));
        Some(crate::vsock_io::VsockIoHandle { tx_rx, rx_tx })
    } else {
        None
    };
```

Also append to the kernel cmdline when vsock is enabled:
```rust
if config.guest_agent != hitz_api::GuestAgentMode::Disabled {
    let vsock_base = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE * 2;
    let _ = write!(
        cmdline,
        " virtio_mmio.device=0x{VIRTIO_MMIO_SIZE:x}@0x{vsock_base:x}:{VIRTIO_IRQ_VSOCK}"
    );
}
```

Create `crates/hitz-vmm/src/vsock_io.rs`:

```rust
//! Channel handle bridging the vsock virtio device (sync run-loop)
//! with the host-side async metrics runtime.

use hitz_api::MetricsSnapshot;

use crate::vsock_device::{VsockHdr, VsockPacket};

/// Owned channel endpoints for the host side of a vsock device.
///
/// Drop to close the channels and signal the run-loop that vsock I/O
/// should stop (device's `try_send`/`try_recv` will return `Err`).
pub struct VsockIoHandle {
    /// Receive guest→host packets (TX queue contents).
    pub tx_rx: crossbeam_channel::Receiver<(hitz_devices::virtio::vsock::VsockHdr,
                                             Vec<u8>)>,
    /// Send host→guest packets (to be injected into RX queue).
    pub rx_tx: crossbeam_channel::Sender<(hitz_devices::virtio::vsock::VsockHdr,
                                           Vec<u8>)>,
}
```

Add `pub mod vsock_io;` to `crates/hitz-vmm/src/lib.rs`.

Also re-export from lib.rs:
```rust
pub use vsock_io::VsockIoHandle;
```

**Note**: `boot_and_run` returns `VmRunResult`. To expose the vsock handle to the daemon, we need to either:
- Return it alongside the result, or
- Store it in a shared `Arc<Mutex<Option<VsockIoHandle>>>` passed in

The cleanest approach: add `vsock: Option<VsockIoHandle>` to `VmRunResult`:

```rust
pub struct VmRunResult {
    pub exit_reason: ExitReason,
    // VsockIoHandle is dropped when the VM exits; this field is always None
    // after boot_and_run returns, but the daemon accesses it during execution
    // via Arc<Mutex<Option<VsockIoHandle>>> passed in config.
}
```

Actually the problem is that `boot_and_run` is called via `spawn_blocking` — the daemon can't access anything inside the closure until it returns. The correct pattern matches what the net device does: the channel endpoints are created *before* `spawn_blocking` and passed to a separate async task.

**Revised approach**: Extract the channel creation from `boot_and_run` and make it configurable. Change `VmConfig` to pass in pre-created channel endpoints, or add a `VsockChannels` parameter to `boot_and_run`.

Simpler: add an `Arc<Mutex<Option<VsockIoHandle>>>` field to a new `BootContext` struct passed alongside `VmConfig`. But that adds complexity.

**Simplest**: Create channels before `spawn_blocking`, pass the receiver into `boot_and_run` via a new optional field on a new `BootExtras` struct:

```rust
pub struct BootExtras {
    pub vsock_rx_sender: Option<crossbeam_channel::Sender<(VsockHdr, Vec<u8>)>>,
    pub vsock_tx_receiver: Option<crossbeam_channel::Receiver<(VsockHdr, Vec<u8>)>>,
}
```

Add `extras: BootExtras` parameter to `boot_and_run`. The daemon:
1. Creates channels before `spawn_blocking`
2. Passes `BootExtras` into `boot_and_run`
3. Keeps the other channel ends for the metrics task

This avoids the need to pass anything through `VmConfig` and keeps the API clean.

For this task, implement the above pattern: `BootExtras` struct, update `boot_and_run` signature.

### Step 4.3 — Run GREEN

```bash
cargo test -p hitz-vmm -- --nocapture 2>&1 | tail -20
cargo clippy -p hitz-vmm -- -D warnings
cargo fmt --check
```

Fix any compile errors in downstream crates that use `boot_and_run` (hitz-daemon, hitz-cli, hitz-whp tests).

### Step 4.4 — Commit

```bash
git add crates/hitz-vmm/src/vm.rs \
        crates/hitz-vmm/src/vsock_io.rs \
        crates/hitz-vmm/src/lib.rs
git commit -m "feat(vsock): wire VirtioVsockDevice into boot pipeline at MMIO slot 2"
```

---

## Task 5: cpio overlay builder

**Files:**
- Create: `crates/hitz-vmm/src/cpio.rs`
- Modify: `crates/hitz-vmm/src/lib.rs`

**Depends on:** Task 1 (GuestAgentMode)

### Step 5.1 — Write failing tests (RED)

Create `crates/hitz-vmm/src/cpio.rs` with just the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_archive_has_trailer() {
        let archive = CpioBuilder::new().finish();
        // Minimum valid cpio: just the TRAILER!!! entry.
        assert!(archive.len() >= 110); // header(110) for TRAILER
        // Magic bytes at offset 0.
        assert_eq!(&archive[0..6], b"070701");
    }

    #[test]
    fn single_file_roundtrip() {
        let content = b"#!/bin/sh\n/sbin/hitz-agent &\n";
        let archive = CpioBuilder::new()
            .add_file("etc/init.d/S99hitz-agent", content, 0o755)
            .finish();
        // Verify magic.
        assert_eq!(&archive[0..6], b"070701");
        // Archive is non-empty and contains the file content.
        assert!(archive.len() > 110 + content.len());
    }

    #[test]
    fn two_files_both_present() {
        let archive = CpioBuilder::new()
            .add_file("sbin/hitz-agent", b"ELF", 0o755)
            .add_file("etc/init.d/S99hitz-agent", b"#!/bin/sh\n", 0o755)
            .finish();
        // Check both filenames appear in the archive bytes.
        let archive_str = String::from_utf8_lossy(&archive);
        assert!(archive_str.contains("sbin/hitz-agent"));
        assert!(archive_str.contains("S99hitz-agent"));
    }
}
```

Run:
```bash
cargo test -p hitz-vmm cpio -- --nocapture
# Expected: compile error — CpioBuilder not found
```

### Step 5.2 — Implement

Full content of `crates/hitz-vmm/src/cpio.rs`:

```rust
//! Minimal newc (SVR4) cpio archive builder.
//!
//! Produces archives compatible with the Linux kernel's initramfs loader.
//! Reference: `Documentation/driver-api/early-userspace/buffer-format.rst`
//!
//! Format: each entry = 110-byte ASCII header + filename (null-term, 4-byte
//! padded) + file data (4-byte padded). Ends with a TRAILER!!! entry.

/// Builds a newc cpio archive in memory.
pub struct CpioBuilder {
    data: Vec<u8>,
    inode: u32,
}

impl CpioBuilder {
    /// Create an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            inode: 1,
        }
    }

    /// Add a regular file entry.
    ///
    /// * `path` — file path inside the archive (no leading `/`)
    /// * `content` — file bytes
    /// * `mode` — Unix permission bits (e.g. `0o755`)
    #[must_use]
    pub fn add_file(mut self, path: &str, content: &[u8], mode: u32) -> Self {
        self.append_entry(path, content, 0o100_000 | (mode & 0o7777));
        self
    }

    /// Finish the archive by appending the TRAILER!!! entry.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.append_entry("TRAILER!!!", &[], 0);
        self.data
    }

    fn append_entry(&mut self, name: &str, content: &[u8], mode: u32) {
        let namesize = name.len() + 1; // include null terminator
        let filesize = content.len();

        // 110-byte ASCII header.
        let header = format!(
            "070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
            self.inode,   // ino
            mode,         // mode
            0u32,         // uid
            0u32,         // gid
            1u32,         // nlink
            0u32,         // mtime
            filesize,     // filesize
            0u32,         // devmajor
            0u32,         // devminor
            0u32,         // rdevmajor
            0u32,         // rdevminor
            namesize,     // namesize
            0u32,         // check (always 0 for newc)
        );
        assert_eq!(header.len(), 110, "cpio header must be 110 bytes");

        self.inode += 1;
        self.data.extend_from_slice(header.as_bytes());

        // Filename + null terminator, padded to 4-byte boundary.
        self.data.extend_from_slice(name.as_bytes());
        self.data.push(0u8);
        let name_total = 110 + namesize;
        let name_pad = (4 - name_total % 4) % 4;
        self.data.extend(std::iter::repeat_n(0u8, name_pad));

        // File content, padded to 4-byte boundary.
        self.data.extend_from_slice(content);
        let data_pad = (4 - filesize % 4) % 4;
        self.data.extend(std::iter::repeat_n(0u8, data_pad));
    }
}

impl Default for CpioBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

Add `pub mod cpio;` and `pub use cpio::CpioBuilder;` to `crates/hitz-vmm/src/lib.rs`.

### Step 5.3 — Run GREEN

```bash
cargo test -p hitz-vmm cpio -- --nocapture
# Expected: all 3 cpio tests pass

cargo clippy -p hitz-vmm -- -D warnings
cargo fmt --check
```

### Step 5.4 — Commit

```bash
git add crates/hitz-vmm/src/cpio.rs crates/hitz-vmm/src/lib.rs
git commit -m "feat(cpio): newc cpio archive builder for initramfs agent injection"
```

---

## Task 6: `hitz-guest-agent` binary crate

**Files:**
- Create: `crates/hitz-guest-agent/` (new crate)
- Create: `crates/hitz-guest-agent/Cargo.toml`
- Create: `crates/hitz-guest-agent/src/main.rs`
- Create: `crates/hitz-guest-agent/src/proc.rs`
- Modify: `Cargo.toml` (workspace — add to `members`)

**Depends on:** Task 1 (MetricsSnapshot types)

**Note:** This crate compiles for `x86_64-unknown-linux-musl`. It does NOT compile on Windows normally — it's cross-compiled. Unit tests for `/proc` parsing use fixture strings and run on the host.

### Step 6.1 — Create workspace member

Add to `Cargo.toml` `[workspace.members]`:
```toml
"crates/hitz-guest-agent",
```

Create `crates/hitz-guest-agent/Cargo.toml`:
```toml
[package]
name    = "hitz-guest-agent"
version.workspace = true
edition.workspace = true

[[bin]]
name = "hitz-agent"
path = "src/main.rs"

[dependencies]
hitz-api    = { path = "../../crates/hitz-api" }
rmp-serde   = "1"
vsock       = "0.5"     # AF_VSOCK socket support for Linux
serde       = { version = "1", features = ["derive"] }

[dev-dependencies]
# No extra dev deps needed — tests use fixture strings
```

### Step 6.2 — Write failing tests (RED)

Create `crates/hitz-guest-agent/src/proc.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PROC_STAT_FIXTURE: &str =
        "cpu  12345 678 9012 345678 123 0 456 0 0 0\n\
         cpu0 6000 300 4000 170000 60 0 200 0 0 0\n\
         cpu1 6345 378 5012 175678 63 0 256 0 0 0\n";

    const PROC_MEMINFO_FIXTURE: &str =
        "MemTotal:        262144 kB\n\
         MemFree:         131072 kB\n\
         Buffers:          10240 kB\n\
         Cached:           20480 kB\n\
         SwapTotal:            0 kB\n\
         SwapFree:             0 kB\n";

    const PROC_DISKSTATS_FIXTURE: &str =
        "   8   0 vda 100 0 800 20 50 0 400 10 0 30 30\n";

    const PROC_NET_DEV_FIXTURE: &str =
        "Inter-|   Receive                                                |  Transmit\n\
         face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
         eth0:    5000     40    0    0    0     0          0         0     2000      20    0    0    0     0       0          0\n";

    #[test]
    fn parse_cpu_stat() {
        let stats = parse_proc_stat_sample(PROC_STAT_FIXTURE);
        // Should have total + 2 cores.
        assert_eq!(stats.len(), 3);
        // Total idle ticks.
        assert_eq!(stats[0].idle, 345678);
    }

    #[test]
    fn parse_meminfo() {
        let mem = parse_proc_meminfo(PROC_MEMINFO_FIXTURE).expect("parse");
        assert_eq!(mem.total_bytes, 262144 * 1024);
        assert_eq!(mem.free_bytes, 131072 * 1024);
        assert_eq!(mem.swap_total, 0);
    }

    #[test]
    fn parse_diskstats() {
        let disks = parse_proc_diskstats(PROC_DISKSTATS_FIXTURE);
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].name, "vda");
        assert_eq!(disks[0].reads_total, 100);
        assert_eq!(disks[0].read_bytes, 800 * 512); // sectors × 512
    }

    #[test]
    fn parse_net_dev() {
        let nets = parse_proc_net_dev(PROC_NET_DEV_FIXTURE);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].interface, "eth0");
        assert_eq!(nets[0].rx_bytes, 5000);
        assert_eq!(nets[0].tx_bytes, 2000);
    }

    #[test]
    fn cpu_utilisation_from_two_samples() {
        let a = CpuSample { user: 1000, nice: 0, system: 200, idle: 8800, iowait: 0, irq: 0, softirq: 0 };
        let b = CpuSample { user: 1200, nice: 0, system: 250, idle: 8550, iowait: 0, irq: 0, softirq: 0 };
        let pct = cpu_pct(&a, &b);
        // Busy delta = 250, total delta = 1000, so 25%.
        assert!((pct - 25.0_f32).abs() < 1.0);
    }
}
```

Run (on host, these are unit tests of the parsing logic):
```bash
cargo test -p hitz-guest-agent -- --nocapture 2>&1 | head -30
# Expected: compile errors — parse functions not found
```

### Step 6.3 — Implement `proc.rs`

```rust
//! Parsers for Linux /proc virtual filesystem entries.

use hitz_api::{DiskMetrics, MemoryMetrics, NetMetrics};

/// Raw CPU tick sample from one `/proc/stat` line.
#[derive(Debug, Clone, Default)]
pub struct CpuSample {
    pub user:    u64,
    pub nice:    u64,
    pub system:  u64,
    pub idle:    u64,
    pub iowait:  u64,
    pub irq:     u64,
    pub softirq: u64,
}

impl CpuSample {
    /// Total active (non-idle) ticks.
    pub fn active(&self) -> u64 {
        self.user + self.nice + self.system + self.irq + self.softirq
    }

    /// Total ticks (active + idle).
    pub fn total(&self) -> u64 {
        self.active() + self.idle + self.iowait
    }
}

/// Calculate CPU utilisation % between two samples.
#[must_use]
pub fn cpu_pct(prev: &CpuSample, curr: &CpuSample) -> f32 {
    let total_delta = curr.total().saturating_sub(prev.total());
    if total_delta == 0 {
        return 0.0;
    }
    let active_delta = curr.active().saturating_sub(prev.active());
    #[allow(clippy::cast_precision_loss)]
    {
        (active_delta as f32 / total_delta as f32) * 100.0
    }
}

/// Parse `/proc/stat` into per-CPU samples (index 0 = aggregate "cpu" line).
#[must_use]
pub fn parse_proc_stat_sample(content: &str) -> Vec<CpuSample> {
    content
        .lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|line| {
            let nums: Vec<u64> = line
                .split_ascii_whitespace()
                .skip(1)
                .take(7)
                .map(|s| s.parse().unwrap_or(0))
                .collect();
            CpuSample {
                user:    nums.first().copied().unwrap_or(0),
                nice:    nums.get(1).copied().unwrap_or(0),
                system:  nums.get(2).copied().unwrap_or(0),
                idle:    nums.get(3).copied().unwrap_or(0),
                iowait:  nums.get(4).copied().unwrap_or(0),
                irq:     nums.get(5).copied().unwrap_or(0),
                softirq: nums.get(6).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// Parse `/proc/meminfo` into [`MemoryMetrics`].
///
/// # Errors
/// Returns `None` if `MemTotal` is missing.
#[must_use]
pub fn parse_proc_meminfo(content: &str) -> Option<MemoryMetrics> {
    let mut total = 0u64;
    let mut free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in content.lines() {
        let mut parts = line.split_ascii_whitespace();
        let key = parts.next()?;
        let val: u64 = parts.next()?.parse().ok()?;
        match key {
            "MemTotal:"   => total      = val * 1024,
            "MemFree:"    => free       = val * 1024,
            "Buffers:"    => buffers    = val * 1024,
            "Cached:"     => cached     = val * 1024,
            "SwapTotal:"  => swap_total = val * 1024,
            "SwapFree:"   => swap_free  = val * 1024,
            _ => {}
        }
    }

    if total == 0 {
        return None;
    }

    Some(MemoryMetrics {
        total_bytes:   total,
        used_bytes:    total.saturating_sub(free + buffers + cached),
        free_bytes:    free,
        buffers_bytes: buffers,
        cached_bytes:  cached,
        swap_total,
        swap_used:     swap_total.saturating_sub(swap_free),
    })
}

/// Parse `/proc/diskstats` into a list of [`DiskMetrics`].
///
/// Only includes devices with non-zero read or write counts (skips
/// partitions / CD-ROMs with no activity).
#[must_use]
pub fn parse_proc_diskstats(content: &str) -> Vec<DiskMetrics> {
    content
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_ascii_whitespace().collect();
            if cols.len() < 14 {
                return None;
            }
            let name     = cols[2].to_string();
            let reads    = cols[3].parse::<u64>().ok()?;
            let read_sec = cols[5].parse::<u64>().ok()?;
            let writes   = cols[7].parse::<u64>().ok()?;
            let write_sec= cols[9].parse::<u64>().ok()?;
            Some(DiskMetrics {
                name,
                reads_total:  reads,
                writes_total: writes,
                read_bytes:   read_sec * 512,
                write_bytes:  write_sec * 512,
            })
        })
        .collect()
}

/// Parse `/proc/net/dev` into a list of [`NetMetrics`].
///
/// Skips the two header lines and the loopback interface.
#[must_use]
pub fn parse_proc_net_dev(content: &str) -> Vec<NetMetrics> {
    content
        .lines()
        .skip(2) // skip header lines
        .filter_map(|line| {
            let (iface, stats) = line.split_once(':')?;
            let iface = iface.trim().to_string();
            if iface == "lo" {
                return None;
            }
            let cols: Vec<u64> = stats
                .split_ascii_whitespace()
                .map(|s| s.parse().unwrap_or(0))
                .collect();
            Some(NetMetrics {
                interface:  iface,
                rx_bytes:   cols.first().copied().unwrap_or(0),
                rx_packets: cols.get(1).copied().unwrap_or(0),
                rx_errors:  cols.get(2).copied().unwrap_or(0),
                tx_bytes:   cols.get(8).copied().unwrap_or(0),
                tx_packets: cols.get(9).copied().unwrap_or(0),
                tx_errors:  cols.get(10).copied().unwrap_or(0),
            })
        })
        .collect()
}
```

### Step 6.4 — Implement `main.rs`

```rust
//! Hitz guest metrics agent.
//!
//! Runs inside the guest VM. Connects to the host via virtio-vsock
//! and streams resource snapshots.
//!
//! Usage: hitz-agent [push|pull|both]  (default: both)

use std::io::{Read, Write};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hitz_api::{CpuMetrics, MetricsRequest, MetricsSnapshot, VSOCK_METRICS_PORT, VMADDR_CID_HOST};
use vsock::{VsockAddr, VsockListener, VsockStream};

mod proc;
use proc::{CpuSample, cpu_pct, parse_proc_diskstats, parse_proc_meminfo,
           parse_proc_net_dev, parse_proc_stat_sample};

const PUSH_INTERVAL_SECS: u64 = 5;
const TOP_N_PROCS: usize = 10;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn collect_snapshot() -> MetricsSnapshot {
    // Two-sample CPU measurement with 100ms between samples.
    let sample1 = parse_proc_stat_sample(&read_file("/proc/stat"));
    thread::sleep(Duration::from_millis(100));
    let sample2 = parse_proc_stat_sample(&read_file("/proc/stat"));

    let total_pct = sample1.first()
        .zip(sample2.first())
        .map(|(a, b)| cpu_pct(a, b))
        .unwrap_or(0.0);

    let per_core: Vec<f32> = sample1.iter().skip(1)
        .zip(sample2.iter().skip(1))
        .map(|(a, b)| cpu_pct(a, b))
        .collect();

    let load_avg = parse_load_avg(&read_file("/proc/loadavg"));
    let memory = parse_proc_meminfo(&read_file("/proc/meminfo"))
        .unwrap_or_else(|| hitz_api::MemoryMetrics {
            total_bytes: 0, used_bytes: 0, free_bytes: 0,
            buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
        });
    let disks    = parse_proc_diskstats(&read_file("/proc/diskstats"));
    let networks = parse_proc_net_dev(&read_file("/proc/net/dev"));
    let processes = collect_top_procs(TOP_N_PROCS);

    MetricsSnapshot {
        timestamp_ms: now_ms(),
        cpu: CpuMetrics { total_pct, per_core, load_avg },
        memory,
        disks,
        networks,
        processes,
    }
}

fn parse_load_avg(content: &str) -> [f32; 3] {
    let mut parts = content.split_ascii_whitespace();
    let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    [a, b, c]
}

fn collect_top_procs(n: usize) -> Vec<hitz_api::ProcMetrics> {
    // Simplified: read /proc/*/stat, sort by utime+stime descending.
    let mut procs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(pid_str) = path.file_name().and_then(|n| n.to_str()) {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    let stat_path = format!("/proc/{pid}/stat");
                    if let Ok(stat) = std::fs::read_to_string(&stat_path) {
                        if let Some(p) = parse_proc_pid_stat(pid, &stat) {
                            procs.push(p);
                        }
                    }
                }
            }
        }
    }
    procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(n);
    procs
}

fn parse_proc_pid_stat(pid: u32, content: &str) -> Option<hitz_api::ProcMetrics> {
    // Format: pid (name) state ppid ...utime stime... rss
    let open = content.find('(')?;
    let close = content.rfind(')')?;
    let name = content[open + 1..close].to_string();
    let rest: Vec<&str> = content[close + 2..].split_ascii_whitespace().collect();
    let state = rest.first()?.chars().next().unwrap_or('?');
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let rss_pages: u64 = rest.get(21)?.parse().ok()?;
    // cpu_pct is approximate (total ticks / HZ, not a differential here)
    #[allow(clippy::cast_precision_loss)]
    let cpu_pct = (utime + stime) as f32 / 100.0; // rough proxy
    Some(hitz_api::ProcMetrics {
        pid,
        name,
        cpu_pct,
        rss_bytes: rss_pages * 4096,
        state,
    })
}

fn send_snapshot(stream: &mut VsockStream, snap: &MetricsSnapshot) -> std::io::Result<()> {
    let encoded = rmp_serde::to_vec(snap)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = (encoded.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)
}

fn push_loop() {
    loop {
        thread::sleep(Duration::from_secs(PUSH_INTERVAL_SECS));
        let snap = collect_snapshot();
        let addr = VsockAddr::new(VMADDR_CID_HOST as u32, VSOCK_METRICS_PORT);
        if let Ok(mut stream) = VsockStream::connect(&addr) {
            let _ = send_snapshot(&mut stream, &snap);
        }
    }
}

fn pull_server() {
    let listener = match VsockListener::bind_with_cid_port(vsock::VMADDR_CID_ANY, VSOCK_METRICS_PORT) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("hitz-agent: failed to bind vsock: {e}");
            return;
        }
    };
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            // Read the request type (1 byte, 0 = Snapshot).
            let mut req_byte = [0u8; 1];
            if stream.read_exact(&mut req_byte).is_ok() {
                let snap = collect_snapshot();
                let _ = send_snapshot(&mut stream, &snap);
            }
        }
    }
}

fn main() {
    // Run push and pull concurrently.
    let push_handle = thread::spawn(push_loop);
    let pull_handle = thread::spawn(pull_server);
    // Join whichever exits first (both are infinite loops in normal operation).
    let _ = push_handle.join();
    let _ = pull_handle.join();
}
```

Also add `mod proc;` to `main.rs`.

**Note:** `VMADDR_CID_HOST` needs to be accessible from hitz-api — add `pub const VMADDR_CID_HOST: u32 = 2;` to `hitz-api/src/lib.rs`.

### Step 6.5 — Run unit tests (host-side only)

The `/proc` parsing tests run on Windows because they use fixture strings:
```bash
cargo test -p hitz-guest-agent -- --nocapture 2>&1 | tail -10
# Expected: 5 tests pass (proc parsing + cpu utilisation)
```

The binary won't compile for the host target, but tests compile fine.

### Step 6.6 — Commit

```bash
git add crates/hitz-guest-agent/ Cargo.toml
git commit -m "feat(guest-agent): hitz-agent binary with /proc parsers and vsock push/pull"
```

---

## Task 7: Cross-compile agent + inject into initramfs

**Files:**
- Create: `crates/hitz-daemon/build.rs`
- Modify: `crates/hitz-daemon/src/agent.rs` (NEW — embed bytes + cpio builder)
- Modify: `crates/hitz-daemon/src/lib.rs`
- Modify: `crates/hitz-vmm/src/vm.rs` (use overlay in boot)

**Depends on:** Tasks 5 (cpio), 6 (guest agent)

### Step 7.1 — Write failing test (RED)

Add to `crates/hitz-daemon/src/agent.rs` (create file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_is_non_empty_cpio() {
        let overlay = build_agent_overlay(b"ELF_FAKE_BINARY");
        // Must start with newc cpio magic.
        assert_eq!(&overlay[0..6], b"070701");
        // Must be large enough to contain both files + trailer.
        assert!(overlay.len() > 300);
    }

    #[test]
    fn overlay_contains_agent_and_init_script() {
        let overlay = build_agent_overlay(b"AGENT");
        let text = String::from_utf8_lossy(&overlay);
        assert!(text.contains("sbin/hitz-agent"));
        assert!(text.contains("S99hitz-agent"));
    }
}
```

### Step 7.2 — Create `build.rs`

Create `crates/hitz-daemon/build.rs`:

```rust
//! Build script: cross-compile hitz-guest-agent for x86_64-unknown-linux-musl
//! and expose its path via `HITZ_AGENT_BIN`.
//!
//! Prerequisites:
//!   rustup target add x86_64-unknown-linux-musl
//!   cargo install cargo-zigbuild
//!
//! If cross-compilation is unavailable, falls back to `assets/hitz-agent`
//! (a pre-built binary committed to the repo).
//! If neither is available, sets `HITZ_AGENT_BIN` to an empty string and
//! GuestAgentMode::Auto silently degrades to Disabled.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../hitz-guest-agent/src");
    println!("cargo:rerun-if-changed=assets/hitz-agent");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .expect("workspace root");

    let target_bin = workspace_root
        .join("target/x86_64-unknown-linux-musl/release/hitz-agent");

    // Try cargo-zigbuild first.
    let built = Command::new("cargo")
        .args([
            "zigbuild",
            "--release",
            "--target", "x86_64-unknown-linux-musl",
            "-p", "hitz-guest-agent",
        ])
        .current_dir(&workspace_root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if built && target_bin.exists() {
        println!("cargo:rustc-env=HITZ_AGENT_BIN={}", target_bin.display());
        return;
    }

    // Fall back to pre-built asset.
    let asset = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/hitz-agent");
    if asset.exists() {
        println!("cargo:rustc-env=HITZ_AGENT_BIN={}", asset.display());
        return;
    }

    // No agent binary available — degraded mode.
    eprintln!(
        "cargo:warning=hitz-guest-agent not available; \
         GuestAgentMode::Auto will degrade to Disabled. \
         Run: rustup target add x86_64-unknown-linux-musl && cargo install cargo-zigbuild"
    );
    println!("cargo:rustc-env=HITZ_AGENT_BIN=");
}
```

Create `crates/hitz-daemon/assets/.gitkeep` so the directory is tracked:
```bash
mkdir -p crates/hitz-daemon/assets
touch crates/hitz-daemon/assets/.gitkeep
```

### Step 7.3 — Implement `agent.rs`

Create `crates/hitz-daemon/src/agent.rs`:

```rust
//! Guest agent embedding and cpio overlay construction.

use hitz_api::GuestAgentMode;
use hitz_vmm::CpioBuilder;

/// The built-in agent binary, embedded at compile time.
///
/// Empty slice when cross-compilation is unavailable (see `build.rs`).
static AGENT_BYTES: &[u8] = {
    const PATH: &str = env!("HITZ_AGENT_BIN");
    if PATH.is_empty() {
        &[]
    } else {
        include_bytes!(env!("HITZ_AGENT_BIN"))
    }
};

const INIT_SCRIPT: &[u8] = b"#!/bin/sh\n/sbin/hitz-agent &\n";

/// Build a newc cpio overlay containing the guest agent and its init script.
///
/// The overlay can be appended directly to the user's initramfs — the
/// kernel processes concatenated cpio archives left-to-right.
#[must_use]
pub fn build_agent_overlay(agent_bytes: &[u8]) -> Vec<u8> {
    CpioBuilder::new()
        .add_file("sbin/hitz-agent", agent_bytes, 0o755)
        .add_file("etc/init.d/S99hitz-agent", INIT_SCRIPT, 0o755)
        .finish()
}

/// Resolve which agent bytes to use given the config mode.
///
/// Returns `None` when injection should be skipped.
#[must_use]
pub fn resolve_agent_bytes(mode: &GuestAgentMode) -> Option<Vec<u8>> {
    match mode {
        GuestAgentMode::Disabled => None,
        GuestAgentMode::Auto => {
            if AGENT_BYTES.is_empty() {
                tracing::warn!(
                    "GuestAgentMode::Auto: no agent binary available (cross-compilation unavailable); \
                     skipping agent injection"
                );
                None
            } else {
                Some(AGENT_BYTES.to_vec())
            }
        }
        GuestAgentMode::Custom(path) => {
            match std::fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    tracing::warn!(path = %path.display(), "failed to read custom agent: {e}");
                    None
                }
            }
        }
    }
}
```

Add `pub mod agent;` to `crates/hitz-daemon/src/lib.rs`.

### Step 7.4 — Wire into boot pipeline

In `crates/hitz-vmm/src/vm.rs`, modify the initramfs loading step:

The daemon (not `hitz-vmm`) is responsible for appending the overlay. It does this in `vm_manager.rs` before calling `boot_and_run`:

In `crates/hitz-daemon/src/vm_manager.rs`, before `spawn_blocking`, append the overlay:

```rust
// Inject guest agent overlay if configured.
let effective_initramfs = if let Some(agent_bytes) =
    crate::agent::resolve_agent_bytes(&config.guest_agent)
{
    let overlay = crate::agent::build_agent_overlay(&agent_bytes);
    if let Some(ref initramfs_path) = config.initramfs_path {
        match std::fs::read(initramfs_path) {
            Ok(mut base) => {
                base.extend_from_slice(&overlay);
                // Write combined to a temp path and update config.
                let tmp = std::env::temp_dir().join(format!("hitz-initrd-{}.cpio", vm_id));
                if std::fs::write(&tmp, &base).is_ok() {
                    let mut patched = config.clone();
                    patched.initramfs_path = Some(tmp);
                    patched
                } else {
                    config.clone()
                }
            }
            Err(_) => config.clone(),
        }
    } else {
        // No initramfs: use just the overlay as the initramfs.
        let tmp = std::env::temp_dir().join(format!("hitz-initrd-{}.cpio", vm_id));
        if std::fs::write(&tmp, &overlay).is_ok() {
            let mut patched = config.clone();
            patched.initramfs_path = Some(tmp);
            patched
        } else {
            config.clone()
        }
    }
} else {
    config.clone()
};
// Use `effective_initramfs` instead of `config` for boot_and_run.
```

### Step 7.5 — Run GREEN

```bash
cargo test -p hitz-daemon agent -- --nocapture
# Expected: overlay_is_non_empty_cpio ... ok
#           overlay_contains_agent_and_init_script ... ok

cargo test -p hitz-daemon -- --nocapture 2>&1 | tail -10
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 7.6 — Commit

```bash
git add crates/hitz-daemon/build.rs \
        crates/hitz-daemon/src/agent.rs \
        crates/hitz-daemon/src/lib.rs \
        crates/hitz-daemon/assets/.gitkeep \
        crates/hitz-vmm/src/vm.rs
git commit -m "feat(agent): cross-compile + embed guest agent, cpio injection at boot"
```

---

## Task 8: Host-side vsock server + OTel publishing

**Files:**
- Create: `crates/hitz-daemon/src/vsock_server.rs`
- Modify: `crates/hitz-daemon/src/lib.rs`

**Depends on:** Tasks 3, 6

### Step 8.1 — Write failing test (RED)

Add to `crates/hitz-daemon/src/vsock_server.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hitz_api::{CpuMetrics, MemoryMetrics, MetricsSnapshot};

    #[test]
    fn publish_to_otel_does_not_panic() {
        // No OTel provider registered — all metric calls must be no-ops.
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics { total_pct: 50.0, per_core: vec![50.0], load_avg: [0.5, 0.4, 0.3] },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024, used_bytes: 128 * 1024 * 1024,
                free_bytes: 128 * 1024 * 1024, buffers_bytes: 0, cached_bytes: 0,
                swap_total: 0, swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        publish_to_otel("test-vm", &snap);
    }
}
```

Run:
```bash
cargo test -p hitz-daemon vsock_server -- --nocapture
# Expected: compile error — publish_to_otel not found
```

### Step 8.2 — Implement

Create `crates/hitz-daemon/src/vsock_server.rs`:

```rust
//! Host-side virtio-vsock metrics receiver.
//!
//! One task per running VM: reads push snapshots from the guest
//! and publishes them to OpenTelemetry.

use hitz_api::{MetricsSnapshot, VSOCK_METRICS_PORT};
use hitz_devices::virtio::vsock::{VsockHdr, VsockOp, VSOCK_BUF_ALLOC, VMADDR_CID_HOST};
use opentelemetry::KeyValue;
use tokio::sync::watch;

use crossbeam_channel::{Receiver, Sender};

/// Receive loop for a single VM's vsock metrics stream.
///
/// Reads push snapshots from the guest's TX channel (via the virtio device)
/// and publishes them to the global OTel meter. Stops when `shutdown` fires.
///
/// # Arguments
/// * `vm_id`    — VM identifier used as an OTel label
/// * `tx_rx`    — receives packets the guest sent (guest→host direction)
/// * `rx_tx`    — sends packets to inject into the guest RX queue
/// * `shutdown` — watch channel; exits when value becomes `true`
pub async fn run_metrics_task(
    vm_id: String,
    tx_rx: Receiver<(VsockHdr, Vec<u8>)>,
    rx_tx: Sender<(VsockHdr, Vec<u8>)>,
    mut shutdown: watch::Receiver<bool>,
) {
    // State for each active connection.
    let mut fwd_cnt: u32 = 0;

    loop {
        tokio::select! {
            // Poll for guest packets (channel is sync; use spawn_blocking).
            packet = tokio::task::spawn_blocking({
                let tx_rx = tx_rx.clone();
                move || tx_rx.recv()
            }) => {
                match packet {
                    Ok(Ok((hdr, payload))) => {
                        handle_packet(&vm_id, &hdr, &payload, &rx_tx, &mut fwd_cnt);
                    }
                    _ => break, // Channel closed or task error
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
        }
    }
}

/// Handle one incoming packet from the guest.
fn handle_packet(
    vm_id: &str,
    hdr: &VsockHdr,
    payload: &[u8],
    rx_tx: &Sender<(VsockHdr, Vec<u8>)>,
    fwd_cnt: &mut u32,
) {
    let op = VsockOp::from_u16(hdr.op);
    match op {
        Some(VsockOp::Request) => {
            // Guest wants to connect — accept it.
            let resp = hdr.make_response(VsockOp::Response, 0);
            let _ = rx_tx.try_send((resp, vec![]));
        }
        Some(VsockOp::Rw) if hdr.dst_port == VSOCK_METRICS_PORT => {
            // Data packet — try to decode as a MetricsSnapshot.
            if payload.len() >= 4 {
                let len = u32::from_le_bytes(payload[0..4].try_into().unwrap_or([0; 4])) as usize;
                let data = &payload[4..];
                if data.len() >= len {
                    if let Ok(snap) = rmp_serde::from_slice::<MetricsSnapshot>(&data[..len]) {
                        publish_to_otel(vm_id, &snap);
                    }
                }
            }
            // Send credit update.
            *fwd_cnt += hdr.len;
            let credit = hdr.make_response(VsockOp::CreditUpdate, 0);
            let mut credit = credit;
            credit.fwd_cnt = *fwd_cnt;
            credit.buf_alloc = VSOCK_BUF_ALLOC;
            let _ = rx_tx.try_send((credit, vec![]));
        }
        Some(VsockOp::Shutdown) | Some(VsockOp::Rst) => {
            // Acknowledge with RST.
            let rst = hdr.make_response(VsockOp::Rst, 0);
            let _ = rx_tx.try_send((rst, vec![]));
        }
        _ => {}
    }
}

/// Publish a [`MetricsSnapshot`] to the global OTel meter.
///
/// All calls are no-ops when no OTel provider is registered.
pub fn publish_to_otel(vm_id: &str, snap: &MetricsSnapshot) {
    let meter = opentelemetry::global::meter("hitz");
    let labels = [KeyValue::new("vm.id", vm_id.to_string())];

    // CPU.
    meter
        .f64_gauge("hitz.guest.cpu_usage")
        .with_description("Guest overall CPU utilisation percentage")
        .build()
        .record(f64::from(snap.cpu.total_pct), &labels);

    for (i, &pct) in snap.cpu.per_core.iter().enumerate() {
        let core_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("cpu", i.to_string()),
        ];
        meter
            .f64_gauge("hitz.guest.cpu_usage")
            .build()
            .record(f64::from(pct), &core_labels);
    }

    // Memory.
    meter
        .u64_gauge("hitz.guest.memory_used_bytes")
        .with_description("Guest memory in use")
        .build()
        .record(snap.memory.used_bytes, &labels);

    meter
        .u64_gauge("hitz.guest.memory_total_bytes")
        .with_description("Guest total RAM")
        .build()
        .record(snap.memory.total_bytes, &labels);

    // Disks.
    for disk in &snap.disks {
        let disk_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("disk", disk.name.clone()),
        ];
        meter.u64_counter("hitz.guest.disk_read_bytes_total").build()
            .add(disk.read_bytes, &disk_labels);
        meter.u64_counter("hitz.guest.disk_write_bytes_total").build()
            .add(disk.write_bytes, &disk_labels);
    }

    // Networks.
    for net in &snap.networks {
        let net_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("interface", net.interface.clone()),
        ];
        meter.u64_counter("hitz.guest.net_rx_bytes_total").build()
            .add(net.rx_bytes, &net_labels);
        meter.u64_counter("hitz.guest.net_tx_bytes_total").build()
            .add(net.tx_bytes, &net_labels);
    }

    tracing::debug!(
        vm.id = vm_id,
        cpu_pct = snap.cpu.total_pct,
        mem_used = snap.memory.used_bytes,
        "guest metrics snapshot received"
    );
}
```

Add `rmp-serde.workspace = true` to `crates/hitz-daemon/Cargo.toml` under `[dependencies]`.

Add `pub mod vsock_server;` to `crates/hitz-daemon/src/lib.rs`.

### Step 8.3 — Run GREEN

```bash
cargo test -p hitz-daemon vsock_server -- --nocapture
# Expected: publish_to_otel_does_not_panic ... ok

cargo test -p hitz-daemon -- --nocapture 2>&1 | tail -10
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 8.4 — Commit

```bash
git add crates/hitz-daemon/src/vsock_server.rs \
        crates/hitz-daemon/src/lib.rs \
        crates/hitz-daemon/Cargo.toml
git commit -m "feat(vsock): host-side metrics receiver + OTel publishing"
```

---

## Task 9: VmEntry wiring + on-demand pull

**Files:**
- Modify: `crates/hitz-daemon/src/vm_manager.rs`
- Modify: `crates/hitz-daemon/src/router.rs`

**Depends on:** Tasks 4, 7, 8

### Step 9.1 — Write failing test (RED)

Add to the `#[cfg(test)]` block in `vm_manager.rs`:

```rust
#[test]
fn vm_entry_has_vsock_field() {
    // Compile guard: VmEntry must have a vsock_tx field.
    let entry: &VmEntry = unsafe {
        // Never executed; just checks the field exists at compile time.
        std::hint::unreachable_unchecked()
    };
    let _: &Option<crossbeam_channel::Sender<_>> = &entry._vsock_rx_tx;
}
```

Actually a cleaner compile guard — just add a comment test:

```rust
#[test]
fn vsock_pull_channel_accessible() {
    // VmManager must expose a method to send a pull request to a VM.
    let mgr = make_manager();
    mgr.create_vm("v1".into(), make_config()).expect("create");
    // get_snapshot returns None for VMs without vsock (no agent in test config).
    // This just verifies the method exists and compiles.
    let _result = mgr.request_metrics_snapshot("v1");
}
```

Run:
```bash
cargo test -p hitz-daemon vsock_pull -- --nocapture
# Expected: compile error — request_metrics_snapshot not found
```

### Step 9.2 — Implement

In `crates/hitz-daemon/src/vm_manager.rs`:

Add to `VmEntry`:
```rust
/// Channel to send host→guest vsock packets (for on-demand metrics pull).
/// `None` when vsock is disabled or before the device is initialised.
_vsock_rx_tx: Option<crossbeam_channel::Sender<(
    hitz_devices::virtio::vsock::VsockHdr,
    Vec<u8>,
)>>,
/// Channel to receive guest→host vsock packets.
_vsock_tx_rx: Option<crossbeam_channel::Receiver<(
    hitz_devices::virtio::vsock::VsockHdr,
    Vec<u8>,
)>>,
```

In `start_vm`, before the `spawn_blocking`, create vsock channels and store them:

```rust
// Create vsock channels if agent injection is enabled.
let (vsock_rx_tx_for_task, vsock_tx_rx_for_task) =
    if config.guest_agent != hitz_api::GuestAgentMode::Disabled {
        let (tx_sender, tx_receiver) = crossbeam_channel::unbounded();
        let (rx_sender, rx_receiver) = crossbeam_channel::unbounded();
        // Store in VmEntry for on-demand pull.
        if let Ok(mut vms) = self.vms.lock() {
            if let Some(entry) = vms.get_mut(&vm_id) {
                entry._vsock_rx_tx = Some(rx_sender);
                entry._vsock_tx_rx = Some(tx_receiver.clone());
            }
        }
        // Spawn metrics receive task.
        let shutdown_rx = self.shutdown_rx.clone(); // if accessible
        let vm_id_task = vm_id.clone();
        tokio::spawn(crate::vsock_server::run_metrics_task(
            vm_id_task,
            tx_receiver,
            // ...
        ));
        (Some(tx_sender), Some(rx_receiver))
    } else {
        (None, None)
    };
```

**Note:** The channel wiring between `start_vm`, `boot_and_run` (via `BootExtras`), and the metrics task is intricate. The key contract:
- Two crossbeam channels: `(guest_tx_sender, guest_tx_receiver)` and `(host_rx_sender, host_rx_receiver)`
- `VirtioVsockDevice` holds `guest_tx_sender` and `host_rx_receiver`
- `run_metrics_task` holds `guest_tx_receiver` and `host_rx_sender`
- `VmEntry` holds a clone of `host_rx_sender` for on-demand pull

Add `request_metrics_snapshot` to `VmManager`:

```rust
/// Send a pull request to the guest agent and wait for the snapshot.
///
/// Returns `None` if vsock is not available for this VM,
/// or on timeout (3 seconds).
pub fn request_metrics_snapshot(&self, vm_id: &str) -> Option<hitz_api::MetricsSnapshot> {
    use hitz_devices::virtio::vsock::{VsockHdr, VsockOp, VSOCK_TYPE_STREAM, VMADDR_CID_HOST};

    let vms = self.vms.lock().ok()?;
    let entry = vms.get(vm_id)?;
    let rx_tx = entry._vsock_rx_tx.as_ref()?;
    let tx_rx = entry._vsock_tx_rx.as_ref()?;

    // Send a MetricsRequest::Snapshot (1 byte, value 0).
    let req_hdr = VsockHdr {
        src_cid:   VMADDR_CID_HOST,
        dst_cid:   entry.config.as_ref()?.guest_cid as u64,
        src_port:  hitz_api::VSOCK_METRICS_PORT,
        dst_port:  hitz_api::VSOCK_METRICS_PORT,
        len:       1,
        r#type:    VSOCK_TYPE_STREAM,
        op:        VsockOp::Rw as u16,
        flags:     0,
        buf_alloc: hitz_devices::virtio::vsock::VSOCK_BUF_ALLOC,
        fwd_cnt:   0,
    };
    rx_tx.try_send((req_hdr, vec![0u8])).ok()?;

    // Wait for response with 3-second timeout.
    tx_rx.recv_timeout(std::time::Duration::from_secs(3))
        .ok()
        .and_then(|(_hdr, payload)| {
            if payload.len() >= 4 {
                let len = u32::from_le_bytes(payload[0..4].try_into().ok()?) as usize;
                rmp_serde::from_slice::<hitz_api::MetricsSnapshot>(&payload[4..][..len]).ok()
            } else {
                None
            }
        })
}
```

Add `GET /vms/{id}/metrics` to `router.rs`:

```rust
(Method::GET, Some("metrics")) => {
    match manager.request_metrics_snapshot(id) {
        Some(snap) => {
            let body = serde_json::to_vec(&snap)
                .unwrap_or_default();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(full_body(body))
                .unwrap_or_default())
        }
        None => Ok(error_response(StatusCode::SERVICE_UNAVAILABLE,
                                  "vsock metrics not available")),
    }
}
```

### Step 9.3 — Run GREEN

```bash
cargo test -p hitz-daemon -- --nocapture 2>&1 | tail -20
cargo clippy -p hitz-daemon -- -D warnings
cargo fmt --check
```

### Step 9.4 — Commit

```bash
git add crates/hitz-daemon/src/vm_manager.rs \
        crates/hitz-daemon/src/router.rs
git commit -m "feat(vsock): VmEntry vsock channels, request_metrics_snapshot, metrics route"
```

---

## Task 10: `hitz vm metrics` CLI command

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

**Depends on:** Task 9

### Step 10.1 — Write failing test (RED)

Add to the tests block in `main.rs`:

```rust
#[test]
fn metrics_output_formats_snapshot() {
    use hitz_api::{CpuMetrics, DiskMetrics, MemoryMetrics, MetricsSnapshot, NetMetrics};
    let snap = MetricsSnapshot {
        timestamp_ms: 0,
        cpu: CpuMetrics {
            total_pct: 12.5,
            per_core: vec![10.0, 15.0],
            load_avg: [0.42, 0.38, 0.31],
        },
        memory: MemoryMetrics {
            total_bytes: 256 * 1024 * 1024,
            used_bytes: 128 * 1024 * 1024,
            free_bytes: 128 * 1024 * 1024,
            buffers_bytes: 0, cached_bytes: 0,
            swap_total: 0, swap_used: 0,
        },
        disks: vec![DiskMetrics {
            name: "vda".into(),
            reads_total: 100, writes_total: 50,
            read_bytes: 512 * 1024, write_bytes: 256 * 1024,
        }],
        networks: vec![NetMetrics {
            interface: "eth0".into(),
            rx_bytes: 1_048_576, tx_bytes: 524_288,
            rx_packets: 1000, tx_packets: 500,
            rx_errors: 0, tx_errors: 0,
        }],
        processes: vec![],
    };
    let output = format_metrics_snapshot(&snap);
    assert!(output.contains("12.5%"));
    assert!(output.contains("128 MiB / 256 MiB"));
    assert!(output.contains("vda"));
    assert!(output.contains("eth0"));
}
```

Run:
```bash
cargo test -p hitz-cli metrics_output -- --nocapture
# Expected: compile error — format_metrics_snapshot not found
```

### Step 10.2 — Implement

Add `Metrics` variant to the `VmCommand` enum in `main.rs`:

```rust
/// Display live resource metrics for a running VM.
Metrics {
    #[arg(help = "VM ID")]
    id: String,
},
```

Add the formatting function:

```rust
fn format_metrics_snapshot(snap: &hitz_api::MetricsSnapshot) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    // CPU.
    let cores: Vec<String> = snap.cpu.per_core.iter()
        .map(|p| format!("{p:.1}%"))
        .collect();
    let _ = writeln!(
        out,
        "CPU:     total={:.1}%  cores=[{}]  load={:.2}/{:.2}/{:.2}",
        snap.cpu.total_pct,
        cores.join(", "),
        snap.cpu.load_avg[0], snap.cpu.load_avg[1], snap.cpu.load_avg[2],
    );

    // Memory.
    let used_mib = snap.memory.used_bytes / (1024 * 1024);
    let total_mib = snap.memory.total_bytes / (1024 * 1024);
    let _ = writeln!(out, "Memory:  {used_mib} MiB / {total_mib} MiB");

    // Disks.
    for disk in &snap.disks {
        let read_kb = disk.read_bytes / 1024;
        let write_kb = disk.write_bytes / 1024;
        let _ = writeln!(
            out,
            "Disk:    {}  reads={}  writes={}  read={}K  write={}K",
            disk.name, disk.reads_total, disk.writes_total, read_kb, write_kb,
        );
    }

    // Networks.
    for net in &snap.networks {
        let rx_kb = net.rx_bytes / 1024;
        let tx_kb = net.tx_bytes / 1024;
        let _ = writeln!(
            out,
            "Net:     {}  rx={}K  tx={}K  rx_pkt={}  tx_pkt={}",
            net.interface, rx_kb, tx_kb, net.rx_packets, net.tx_packets,
        );
    }

    // Processes.
    if !snap.processes.is_empty() {
        let _ = writeln!(out, "Procs:   {:>6}  {:<20} {:>6}  {:>8}", "PID", "NAME", "CPU%", "RSS");
        for proc in &snap.processes {
            let rss_mb = proc.rss_bytes / (1024 * 1024);
            let _ = writeln!(
                out,
                "         {:>6}  {:<20} {:>5.1}%  {:>7}M",
                proc.pid, proc.name, proc.cpu_pct, rss_mb,
            );
        }
    }

    out
}
```

Add handling in the `VmCommand::Metrics` match arm:

```rust
VmCommand::Metrics { id } => {
    let resp = client.request("GET", &format!("/vms/{id}/metrics"), None)?;
    let snap: hitz_api::MetricsSnapshot = serde_json::from_str(&resp)
        .context("failed to parse metrics response")?;
    print!("{}", format_metrics_snapshot(&snap));
}
```

### Step 10.3 — Run GREEN

```bash
cargo test -p hitz-cli metrics_output -- --nocapture
# Expected: metrics_output_formats_snapshot ... ok

cargo test -p hitz-cli -- --nocapture 2>&1 | tail -10
cargo clippy -p hitz-cli -- -D warnings
cargo fmt --check
```

### Step 10.4 — Commit

```bash
git add crates/hitz-cli/src/main.rs
git commit -m "feat(cli): hitz vm metrics command with formatted output"
```

---

## Task 11: Integration test skeleton

**Files:**
- Modify: `crates/hitz-whp/src/tests.rs`

**Depends on:** Tasks 1–10

### Step 11.1 — Add test

Add after the last test in `crates/hitz-whp/src/tests.rs`:

```rust
/// Phase 12 regression guard: virtio-vsock device initialises correctly
/// and OTel metric types compile and run without panic.
///
/// This is a compile + no-panic test, not a full guest connectivity test.
/// For full connectivity: boot a guest with hitz-agent, verify snapshot received.
///
/// Run with: cargo test -p hitz-whp -- --ignored phase12
#[test]
#[ignore]
fn phase12_vsock_noop() {
    use hitz_devices::virtio::vsock::{VirtioVsockDevice, VsockHdr, VsockOp, VSOCK_TYPE_STREAM};

    // Construct the device.
    let (device, tx_rx, rx_tx) = VirtioVsockDevice::new(3);
    assert_eq!(device.device_id(), 19);
    assert_eq!(device.queue_count(), 3);

    // Verify channel communication works.
    let hdr = VsockHdr {
        src_cid: 3, dst_cid: 2,
        src_port: 12345, dst_port: 52355,
        len: 0,
        r#type: VSOCK_TYPE_STREAM,
        op: VsockOp::Request as u16,
        flags: 0, buf_alloc: 262_144, fwd_cnt: 0,
    };
    rx_tx.try_send((hdr, vec![])).expect("send");
    let (received_hdr, _) = tx_rx.try_recv().expect("recv");
    // rx_tx → device rx queue → device processes → tx_sender → tx_rx
    // For this test just check the echo from the test channel endpoints.
    assert_eq!(received_hdr.src_cid, 3);

    // OTel no-op path.
    let snap = hitz_api::MetricsSnapshot {
        timestamp_ms: 0,
        cpu: hitz_api::CpuMetrics { total_pct: 0.0, per_core: vec![], load_avg: [0.0; 3] },
        memory: hitz_api::MemoryMetrics {
            total_bytes: 0, used_bytes: 0, free_bytes: 0,
            buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0,
        },
        disks: vec![], networks: vec![], processes: vec![],
    };
    hitz_daemon::vsock_server::publish_to_otel("test-vm", &snap);

    eprintln!("phase12_vsock_noop: vsock device and OTel path verified");
}
```

### Step 11.2 — Run

```bash
cargo build --workspace 2>&1 | tail -5
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --workspace -- -D warnings 2>&1 | tail -5
cargo fmt --check

# On WHP machine:
cargo test -p hitz-whp -- --ignored phase12 --nocapture --test-threads=1
```

### Step 11.3 — Commit

```bash
git add crates/hitz-whp/src/tests.rs
git commit -m "test(vsock): phase12 vsock noop regression guard"
```

---

## Final verification

```bash
# Full build
cargo build --workspace 2>&1 | tail -3

# All non-ignored tests
cargo test --workspace 2>&1 | grep -E "^test result"

# Clippy clean
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# Format clean
cargo fmt --check
```
