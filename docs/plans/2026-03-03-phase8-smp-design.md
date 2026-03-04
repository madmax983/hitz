# Phase 8: Multi-vCPU SMP

## Goal

Add symmetric multiprocessing support so `hitz run --kernel vmlinux --cpus N` boots Linux with N virtual CPUs.

## Architecture

Three layers of work: (1) ACPI tables for CPU discovery, (2) per-vCPU threading with shared device state, (3) exit coordination.

WHP's built-in xAPIC emulation handles IPI delivery between vCPUs — when the BSP writes to the LAPIC ICR register to send INIT+STARTUP IPIs, WHP delivers them to the target AP. APs start in wait-for-SIPI state and wake at the trampoline address encoded in the SIPI vector.

## ACPI Tables

Linux discovers CPUs via the ACPI MADT (Multiple APIC Description Table). We build three minimal tables in a new `hitz-boot/src/acpi.rs` module:

**RSDP** (Root System Description Pointer) — 36 bytes at `0x000E_0000`. Contains signature `"RSD PTR "`, revision 2, pointer to XSDT, checksums.

**XSDT** (Extended System Description Table) — at `0x000E_1000`. Header + one 64-bit pointer to the MADT.

**MADT** (Multiple APIC Description Table) — at `0x000E_2000`. Lists Local APIC address (`0xFEE0_0000`), then N entries of type 0 (Processor Local APIC), each 8 bytes: `(type=0, len=8, acpi_proc_id, apic_id, flags=1)`.

All tables are written to guest memory at boot time. `boot_params.acpi_rsdp_addr` is set to `0x000E_0000`.

### GPA Layout Addition

```
0x000E_0000  RSDP (36 bytes)
0x000E_1000  XSDT (header + pointer)
0x000E_2000  MADT (header + N LAPIC entries)
```

## VmConfig Changes

- Add `cpus: u32` to `VmConfig` with default 1 (`DEFAULT_CPUS`)
- Add `--cpus <N>` flag to CLI `RunArgs` and `VmCreateArgs`
- `PartitionConfig.vcpu_count` uses `config.cpus`

## Threading Model

### Per-vCPU Threads

`boot_and_run` spawns one `std::thread` per vCPU. Each thread runs `run_vcpu_loop` with shared state.

- BSP (vCPU 0): configured with kernel entry point + boot_params (as today)
- APs (vCPU 1+): no register setup — WHP starts them in wait-for-SIPI state

### Shared State

```rust
struct SharedDevices<W: Write> {
    serial: SerialDevice<W>,
    mmio_bus: MmioBus,
}
```

Wrapped in `Arc<Mutex<SharedDevices<W>>>`. Each vCPU thread locks on I/O exits only. Contention is negligible — I/O exits are ~0.01% of total exits, lock hold time is microseconds.

Guest memory (`Arc<GuestMemory>`) is already shared without locks (read-only after boot for the VMM; guest writes go through the hypervisor's memory mapping).

### Exit Coordination

- Shared `Arc<AtomicBool>` stop flag
- When **any** vCPU hits `Halt`, `Shutdown`, or error → sets the stop flag
- All other vCPUs see it on next iteration → return `Canceled`
- Main thread joins all vCPU threads, reports the first exit reason
- External stop (Ctrl+C, daemon stop) works unchanged via the same flag

### run_vcpu_loop Signature Change

```rust
pub fn run_vcpu_loop<V: Vcpu, W: Write + Send>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    stop_flag: &AtomicBool,
) -> Result<ExitReason, HalError>
```

The `Option<&AtomicBool>` becomes `&AtomicBool` — `boot_and_run` always provides one (creates a local one when the caller doesn't supply one).

## Testing

**Unit tests:**
- ACPI table construction: RSDP checksum, XSDT pointer, MADT LAPIC entries for N=1,2,4
- VmConfig serde with cpus field

**WHP integration tests (#[ignore]):**
- `phase8_smp_2vcpu_hello`: 2-vCPU partition, BSP runs hello ELF, verify clean exit
- `phase8_smp_linux_boot`: Full Linux boot with `--cpus 2`, verify kernel serial output shows `smp: Brought up 1 node, 2 CPUs`. Gated on `HITZ_KERNEL_PATH`.

## Files Changed

| File | Action | Purpose |
|------|--------|---------|
| `hitz-boot/src/acpi.rs` | NEW | RSDP, XSDT, MADT construction |
| `hitz-boot/src/lib.rs` | Modify | Add `pub mod acpi`, re-exports, ACPI GPA constants |
| `hitz-api/src/lib.rs` | Modify | Add `cpus: u32` to VmConfig, DEFAULT_CPUS |
| `hitz-vmm/src/run_loop.rs` | Modify | SharedDevices struct, new signature |
| `hitz-vmm/src/vm.rs` | Modify | Multi-vCPU creation, thread spawning, ACPI table writing |
| `hitz-vmm/src/lib.rs` | Modify | Re-export SharedDevices if needed |
| `hitz-cli/src/main.rs` | Modify | --cpus flag |
| `hitz-whp/src/tests.rs` | Modify | Phase 8 integration tests |

## What We're NOT Building

- No NUMA topology (single node)
- No CPU hotplug
- No per-vCPU LAPIC timer emulation (WHP handles it)
- No x2APIC (xAPIC is sufficient for < 255 CPUs)
- No IOAPIC emulation (virtio uses MSI-like transport-level IRQs)
