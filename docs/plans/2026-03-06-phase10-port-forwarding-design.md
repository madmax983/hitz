# Phase 10: Port Forwarding — Design

## Goal

Expose guest TCP services to the host via static port forward rules declared at VM creation time. `hitz run --port 2222:22` makes SSH into a guest work from the host.

## Scope

**In:** Static TCP host→guest forwarding, CLI flag, daemon API, status display.
**Out:** UDP, dynamic add/remove on running VM, port conflict detection across VMs, privileged ports (<1024) enforcement, reverse (guest→host) forwarding, connection metrics.

## Data Model

New type in `hitz-api/src/lib.rs`:

```rust
/// A single TCP port forward rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortForward {
    /// Port to listen on the host (e.g. 2222).
    pub host_port: u16,
    /// Port to connect to in the guest (e.g. 22).
    pub guest_port: u16,
}
```

`VmConfig` gains:

```rust
/// TCP port forwards: host_port → guest_port. Empty = no forwarding.
#[serde(default)]
pub ports: Vec<PortForward>,
```

`#[serde(default)]` ensures existing serialized configs without the field deserialize to an empty vec.

## TCP Proxy Architecture

`PortForwardManager` in `hitz-daemon/src/port_forward.rs`:

```rust
pub struct PortForwardManager {
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl PortForwardManager {
    pub async fn start(guest_ip: Ipv4Addr, rules: &[PortForward]) -> Self;
    // Drop impl calls abort() on all handles.
}
```

**Per-port listener loop:**

```
TcpListener::bind(0.0.0.0:host_port)
  └─ loop: accept()
       └─ spawn: copy_bidirectional(inbound, TcpStream::connect(guest_ip:guest_port))
```

`tokio::io::copy_bidirectional` handles full-duplex relay with no manual buffering. Each accepted connection runs in its own task. Bind failures are logged as warnings and skipped — they do not block VM boot.

## Daemon Integration

`VmEntry` gains `port_fwd: Option<PortForwardManager>`.

In `start_vm`, after state is set to Running, before spawning `boot_and_run`:

1. Parse `guest_ip` from `config.net.guest_ip` (already done for WinTun setup).
2. If `config.net.is_some() && !config.ports.is_empty()` → call `PortForwardManager::start(guest_ip, &config.ports).await`.
3. Store in `entry.port_fwd`.

When the VM stops (spawned task completes), `port_fwd` is dropped → `PortForwardManager::drop` aborts all listener handles → all listener tasks exit within one `accept()` poll.

If `config.net` is `None`, `ports` is ignored (no guest IP to forward to).

## CLI Integration

`--port HOST:GUEST` flag (repeatable) on `hitz run` and `hitz vm create`:

```
hitz run --kernel vmlinux --port 2222:22 --port 8080:80
```

Parser:

```rust
fn parse_port_forward(s: &str) -> Result<PortForward, String> {
    let (host, guest) = s.split_once(':')
        .ok_or_else(|| format!("expected HOST:GUEST, got {s:?}"))?;
    Ok(PortForward {
        host_port: host.parse().map_err(|_| format!("invalid host port: {host}"))?,
        guest_port: guest.parse().map_err(|_| format!("invalid guest port: {guest}"))?,
    })
}
```

`hitz vm status` adds a Ports line when `config.ports` is non-empty:

```
ID:     vm-abc123
State:  Running
Ports:  0.0.0.0:2222 → 22, 0.0.0.0:8080 → 80
```

## Files Changed

| File | Change |
|------|--------|
| `hitz-api/src/lib.rs` | `PortForward` struct, `ports` field on `VmConfig` |
| `hitz-daemon/src/port_forward.rs` | NEW: `PortForwardManager` |
| `hitz-daemon/src/lib.rs` | `pub mod port_forward` |
| `hitz-daemon/src/vm_manager.rs` | `VmEntry.port_fwd`, start/stop in `start_vm` |
| `hitz-cli/src/main.rs` | `--port` flag, `parse_port_forward`, status display |

## Testing

**Unit (no WHP):**
- `parse_port_forward`: valid, missing colon, non-numeric, port 0
- `PortForward` serde roundtrip: empty vec, single rule, multiple rules
- `VmConfig` with `ports`: `#[serde(default)]` round-trips correctly for old configs
- `PortForwardManager`: bind conflict on same host port logs warning, other rules still start

**Integration (`#[ignore]`, WHP + networking):**
- `phase10_port_forward_tcp`: boot Linux guest with TCP echo server, connect from host via forwarded port, verify echo

## Connection Flow

```
Host process
  └─ TCP connect to 127.0.0.1:2222
       └─ PortForwardManager listener (host port 2222)
            └─ copy_bidirectional
                 └─ TcpStream::connect(192.168.100.2:22)
                      └─ Guest sshd
```

Round-trip: host process ↔ tokio relay task ↔ WinTun ↔ virtio-net ↔ guest kernel.
