# Hitz — Hyper-V MicroVM Manager

Firecracker's spiritual successor on Windows. A Rust microVM manager using the Windows Hypervisor Platform (WHP) API.

## Architecture

```
hitz-cli -> hitz-api
hitz-daemon -> hitz-vmm -> hitz-hal (trait)
           -> hitz-api     -> hitz-boot -> linux-loader, vm-memory
           -> hitz-net     -> hitz-devices -> virtio-queue, vm-memory
hitz-whp -> hitz-hal (implements)
         -> windows-rs
```

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `hitz-hal` | Hypervisor Abstraction Layer — traits only, no platform deps |
| `hitz-whp` | WHP backend implementing hitz-hal via windows-rs |
| `hitz-vmm` | VM lifecycle, vCPU threads, memory management |
| `hitz-boot` | Linux direct boot (ELF loader, boot_params, page tables) |
| `hitz-devices` | MMIO/IO bus, virtio-mmio transport, serial, block |
| `hitz-net` | virtio-net + WinTun glue |
| `hitz-api` | REST types, VmAction enum (no I/O) |
| `hitz-daemon` | tokio runtime, named pipe server, VM coordinator |
| `hitz-cli` | CLI binary (clap + named pipe client) |

## Key Design Decisions

- **WHP (low-level)** over Hyper-V WMI — full VMM control, closest to KVM
- **Direct kernel boot** — no BIOS/UEFI, fastest path
- **Linux first** — simpler boot protocol validates VMM before Windows guests
- **Named pipe REST API** — secure default, TCP opt-in for dev

## WHP vs KVM Differences

| Feature | KVM | WHP | Impact |
|---------|-----|-----|--------|
| ioeventfd | Yes | No | Every virtio notify exits to userspace |
| irqfd | Yes | No | Must cancel vCPU + use interrupt window |
| MMIO decode | Kernel fills address/data/size | Raw instruction bytes | Use WinHvEmulation.dll |
| Networking | Linux TAP | WinTun adapter | Different crate |

## Known Issues

- `vm-memory` crate doesn't compile on Windows (libc read/write c_uint vs usize mismatch)
- Will need custom guest memory implementation using VirtualAlloc + WHvMapGpaRange
- rust-vmm crates are Linux-focused; expect Windows compat work in Phases 1-3
- **WHV_REGISTER_VALUE union init**: `{ Reg64: val }` only writes 8 of 16 bytes. WHP reads all 16. Use `reg64_val()` helper in `convert.rs` which writes via `Reg128` to zero-extend.
- **WHP tests must run serially**: `--test-threads=1` required (parallel partition creation causes 0xC0370008)

## Building

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

## Guest Physical Address Layout

```
0x00000000 - 0x0009FFFF  Conventional RAM (640 KB)
0x00007000               boot_params (zero page)
0x00008000 - 0x0000BFFF  Page tables (PML4, PDPTE, PDE)
0x00020000               Kernel command line
0x00100000 - RAM_END     Main RAM (kernel loads at 1 MiB)
0xD0000000 - 0xD0FFFFFF  virtio-MMIO slots (4 KB each, IRQs 5+)
```
