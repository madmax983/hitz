# Phase 7: TCP Listener, Serial Streaming, virtio-net

## Goal

Three features: (1) opt-in TCP listener for remote daemon management, (2) chunked HTTP serial console streaming, (3) virtio-net device with WinTun adapter and bridged networking via L2↔L3 header translation.

## Architecture

```
+------------------------------------------------------------------+
|  hitz daemon start [--tcp-listen 127.0.0.1:50505]                |
|                                                                   |
|  +-----------+    +-----------+    +------------------------+     |
|  | Named Pipe|    | TCP       |    |  HTTP Router (hyper)   |     |
|  | Listener  |--->| Listener  |--->|  existing + serial EP  |     |
|  +-----------+    +-----------+    +-----------+------------+     |
|                                                |                  |
|                                     +----------v-----------+     |
|                                     |    VmManager           |     |
|                                     +----------+------------+     |
|                                                |                  |
|              +---------------------------------+-------+          |
|              v                                         v          |
|   +--------------------+                    +--------------------+|
|   |  VM #1             |                    |  VM #2             ||
|   |  SerialBuf ←serial |                    |  SerialBuf ←serial ||
|   |  VirtioNet ←→ I/O  |                    |  (no net)          ||
|   |  thread ←→ WinTun  |                    +--------------------+|
|   +--------------------+                                          |
+------------------------------------------------------------------+

Guest TX: virtio TX queue → Ethernet frame → strip L2 → IP packet → WinTun
Guest RX: WinTun → IP packet → prepend L2 header → virtio RX queue → guest
ARP:      Handled in userspace (synthetic replies for gateway MAC)
```

## Design Decisions

- **TCP opt-in** — `--tcp-listen <addr>` flag. Named pipe always on. Secure default, TCP for dev/remote use.
- **Chunked HTTP for serial** — `GET /vms/{id}/serial` returns `Transfer-Encoding: chunked`. Works with curl. No WebSocket complexity.
- **SerialBuf ring buffer** — shared `Arc<Mutex<RingBuf>>` with `Notify`. Multiple readers get independent cursors. 64 KB default.
- **WinTun (L3)** — not TAP (L2). Strip/inject 14-byte Ethernet headers in userspace. Handle ARP locally.
- **Dedicated I/O thread** — polls WinTun + checks crossbeam channels. Doesn't block vCPU loop.
- **Static IPs only** — no DHCP server. Guest configured via kernel `ip=` cmdline parameter.
- **Auto-configure host IP** — `netsh interface ip set address` at VM start. WHP already needs admin.
- **crossbeam channels** — `tx_chan` (guest→host) and `rx_chan` (host→guest) between virtio-net device and I/O thread.

## TCP Listener

Named pipe listener loop unchanged. TCP listener runs as a parallel `tokio::spawn` task:

```rust
// In server.rs
if let Some(addr) = tcp_addr {
    let listener = TcpListener::bind(addr).await?;
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let mgr = manager.clone();
            tokio::spawn(async move {
                http1::Builder::new()
                    .serve_connection(io, service_fn(|req| route(req, &mgr)))
                    .await
            });
        }
    });
}
```

CLI: `hitz daemon start --tcp-listen 127.0.0.1:50505`
Client: `hitz vm list --tcp 127.0.0.1:50505`

## Serial Streaming

### SerialBuf

```rust
pub struct SerialBuf {
    buf: Vec<u8>,          // fixed 64 KB ring
    write_pos: usize,
    notify: Arc<Notify>,
}

impl Write for SerialBuf { /* append bytes, wrap, notify */ }

pub struct SerialReader {
    buf: Arc<Mutex<SerialBuf>>,
    read_pos: usize,
    notify: Arc<Notify>,
}

impl SerialReader {
    pub async fn read_chunk(&mut self) -> Vec<u8> { /* wait for notify, drain from read_pos */ }
}
```

### API Endpoint

`GET /vms/{id}/serial` — returns chunked HTTP response. Router creates a `SerialReader` from the VM's `SerialBuf`, yields chunks as bytes arrive. Connection close = reader dropped.

### CLI

`hitz vm serial <ID>` — connects to serial endpoint, prints to stdout. Like `docker logs -f`.

## virtio-net Device

### VirtioNetDevice

```rust
pub struct VirtioNetDevice {
    mac: [u8; 6],
    tx_sender: crossbeam_channel::Sender<Vec<u8>>,    // frames to I/O thread
    rx_receiver: crossbeam_channel::Receiver<Vec<u8>>, // frames from I/O thread
    rx_pending: VecDeque<Vec<u8>>,                     // waiting for RX queue space
}

impl VirtioBackend for VirtioNetDevice {
    fn device_type(&self) -> u32 { 1 }  // network
    fn config_space(&self) -> Vec<u8> { self.mac.to_vec() }  // 6-byte MAC
    fn queue_count(&self) -> usize { 2 }  // RX=0, TX=1
    fn queue_notify(&mut self, queue: u16, ...) { ... }
}
```

Feature bits: `VIRTIO_NET_F_MAC` (bit 5). virtio-net header (12 bytes, all zeros — no offload).

### MMIO Registration

```
Slot 0: 0xD000_0000 — virtio-blk (IRQ 5)  [existing]
Slot 1: 0xD000_1000 — virtio-net (IRQ 6)  [new]
```

Kernel cmdline: `virtio_mmio.device=0x1000@0xD0001000:6`

## Network I/O Thread

```rust
fn net_io_thread(
    session: wintun::Session,
    tx_receiver: Receiver<Vec<u8>>,  // guest→host frames
    rx_sender: Sender<Vec<u8>>,      // host→guest frames
    guest_mac: [u8; 6],
    gateway_mac: [u8; 6],
    stop_flag: Arc<AtomicBool>,
) {
    loop {
        if stop_flag.load(Relaxed) { break; }

        // Check for guest TX packets
        if let Ok(frame) = tx_receiver.try_recv() {
            match ethertype(&frame) {
                ARP => handle_arp(&frame, &rx_sender, guest_mac, gateway_mac),
                IPv4 | IPv6 => {
                    let ip_packet = &frame[14..]; // strip Ethernet header
                    session.send_packet(ip_packet);
                }
            }
        }

        // Check for host RX packets
        if let Some(packet) = session.try_receive() {
            let frame = prepend_ethernet_header(&packet, gateway_mac, guest_mac);
            let _ = rx_sender.send(frame);
        }
    }
}
```

### ARP Handling

When guest ARPs for the gateway IP, we synthesize a reply:
- Opcode: 2 (reply)
- Sender MAC: `gateway_mac` (fake, e.g. `AA:BB:CC:00:00:01`)
- Sender IP: gateway IP (e.g. `192.168.100.1`)
- Target MAC/IP: guest's MAC/IP

### Host-Side IP Configuration

At VM start, after creating the WinTun adapter:
```
netsh interface ip set address "hitz-{vm_id}" static 192.168.100.1 255.255.255.0
```

## VmConfig Extension

```rust
pub struct NetConfig {
    pub mac: Option<String>,           // default: random
    pub host_ip: String,               // e.g. "192.168.100.1/24"
    pub guest_ip: String,              // e.g. "192.168.100.2/24"
    pub adapter_name: Option<String>,  // default: "hitz-{vm_id}"
}

pub struct VmConfig {
    pub kernel_path: PathBuf,
    pub initramfs_path: Option<PathBuf>,
    pub disk_path: Option<PathBuf>,
    pub ram_mib: u32,
    pub cmdline: Option<String>,
    pub net: Option<NetConfig>,  // NEW
}
```

Guest cmdline auto-appended when `net` is present:
`ip=192.168.100.2::192.168.100.1:255.255.255.0::eth0:off`

## CLI Changes

```
hitz run --kernel <PATH> [--net --host-ip <CIDR> --guest-ip <CIDR> --mac <MAC>]
hitz vm create <ID> --kernel <PATH> [--net --host-ip <CIDR> --guest-ip <CIDR>]
hitz vm serial <ID>           # NEW: stream serial output
hitz daemon start [--tcp-listen <ADDR>]  # NEW: opt-in TCP
hitz vm list [--tcp <ADDR>]   # NEW: --tcp flag on all vm commands
```

## Task Breakdown

### Task 1: TCP listener (independent)
- Add `TcpListener` loop to `server.rs`
- `--tcp-listen` flag on `daemon start`
- `--tcp` flag on `hitz vm *` commands
- TCP client mode in `pipe_client.rs`

### Task 2: SerialBuf ring buffer (independent)
- `hitz-vmm/src/serial_buf.rs` — ring buffer with `Write` impl, `Notify`, `SerialReader`
- Unit tests for wrap-around, multiple readers, notify

### Task 3: Serial streaming endpoint (depends on 2)
- Wire `SerialBuf` into `VmManager::start_vm` (replace log file)
- `GET /vms/{id}/serial` chunked response in router
- `hitz vm serial <ID>` CLI command

### Task 4: NetConfig + API types (independent)
- `NetConfig` struct in `hitz-api`
- `VmConfig.net` field
- CLI flags for `hitz run` and `hitz vm create`
- Serde roundtrip tests

### Task 5: virtio-net device (depends on 4)
- `hitz-devices/src/virtio/net.rs` — `VirtioNetDevice` implementing `VirtioBackend`
- TX/RX queue handling, virtio-net header
- crossbeam channel setup
- Unit tests with mock channels

### Task 6: WinTun I/O thread + Ethernet handling (depends on 5)
- `hitz-net/src/wintun_io.rs` — I/O thread, adapter lifecycle
- `hitz-net/src/ethernet.rs` — header strip/inject, ARP responder
- `netsh` host IP configuration
- Unit tests for Ethernet parsing, ARP reply generation

### Task 7: Integration into boot_and_run (depends on 5, 6)
- MMIO slot 1 registration when `NetConfig` present
- Cmdline generation (`virtio_mmio.device=...`, `ip=...`)
- I/O thread spawn + stop flag wiring
- Shutdown cleanup (WinTun adapter)

### Task 8: Integration tests (depends on 3, 7)
- Serial streaming: start VM, read chunked serial output, verify content
- virtio-net: create adapter, verify host-side IP, basic packet flow (WHP-gated)

## Dependency Graph

```
Task 1 (TCP) ─────────────────────────────────────────────────┐
Task 2 (SerialBuf) ──→ Task 3 (serial endpoint) ─────────────┤
Task 4 (NetConfig) ──→ Task 5 (virtio-net) ──→ Task 7 (boot) ├→ Task 8 (tests)
                                    │                          │
                                    └──→ Task 6 (WinTun I/O) ─┘
```

## What We're NOT Building

- No DHCP server (static IPs only)
- No virtio-net multiqueue
- No checksum/GSO/TSO offload (all zeros in virtio-net header)
- No WebSocket for serial streaming
- No Windows service registration
- No multi-vCPU support
- No IPv6 routing (passthrough only)
