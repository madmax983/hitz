# Phase 8: Multi-vCPU SMP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Boot Linux with N virtual CPUs using ACPI MADT for CPU discovery and per-vCPU OS threads.

**Architecture:** Build ACPI RSDP/XSDT/MADT tables in `hitz-boot`, expose `acpi_rsdp_addr` in boot_params, refactor the run loop to use `Arc<Mutex<SharedDevices>>`, spawn N vCPU threads in `boot_and_run`, coordinate exits via a shared stop flag.

**Tech Stack:** Rust, WHP xAPIC emulation (handles IPI delivery between vCPUs), ACPI 2.0 table format.

---

## Dependency Graph

```
Task 1 (ACPI tables) ──► Task 2 (boot_params field) ──► Task 5 (multi-vCPU boot)
Task 3 (VmConfig cpus)  ──────────────────────────────► Task 5
Task 4 (SharedDevices)   ──────────────────────────────► Task 5
Task 5 ──► Task 6 (CLI --cpus)
Task 5 ──► Task 7 (integration tests)
```

Tasks 1, 3, 4 are independent of each other. Task 2 depends on Task 1. Tasks 5+ depend on 2, 3, 4.

---

## Task 1: ACPI Table Construction

**Files:**
- Create: `crates/hitz-boot/src/acpi.rs`
- Modify: `crates/hitz-boot/src/lib.rs`

**What we're building:** Three ACPI tables (RSDP, XSDT, MADT) that Linux parses during early boot to discover how many CPUs exist. Each table is a byte array written to a fixed GPA in guest memory.

### Step 1: Create acpi.rs with GPA constants and ACPI structures

Create `crates/hitz-boot/src/acpi.rs`:

```rust
//! Minimal ACPI table construction for SMP CPU discovery.
//!
//! Builds RSDP → XSDT → MADT so Linux can enumerate multiple CPUs
//! via the MADT's Local APIC entries. These tables are written to
//! guest memory during boot.

use crate::error::BootError;

/// GPA where the RSDP is placed (within the BIOS read-only area).
pub const RSDP_GPA: u64 = 0x000E_0000;
/// GPA where the XSDT is placed.
pub const XSDT_GPA: u64 = 0x000E_1000;
/// GPA where the MADT is placed.
pub const MADT_GPA: u64 = 0x000E_2000;

/// Standard Local APIC physical address.
const LOCAL_APIC_ADDRESS: u32 = 0xFEE0_0000;

/// ACPI table checksum: sum of all bytes in the table must be 0 (mod 256).
fn acpi_checksum(data: &[u8]) -> u8 {
    let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    (!sum).wrapping_add(1)
}

/// Build the ACPI RSDP (Root System Description Pointer), revision 2.
///
/// Returns a 36-byte RSDP structure pointing to the XSDT at [`XSDT_GPA`].
pub fn build_rsdp() -> [u8; 36] {
    let mut rsdp = [0u8; 36];

    // Signature: "RSD PTR " (8 bytes, space-padded)
    rsdp[0..8].copy_from_slice(b"RSD PTR ");

    // Checksum (byte 8) — computed below.

    // OEM ID (bytes 9..15): "HITZ  " (6 bytes, space-padded)
    rsdp[9..15].copy_from_slice(b"HITZ  ");

    // Revision (byte 15): 2 = ACPI 2.0+ (uses XSDT)
    rsdp[15] = 2;

    // RSDT Address (bytes 16..20): 0 — we only use XSDT.
    // (left as zero)

    // Length (bytes 20..24): 36 = total RSDP size for revision 2.
    rsdp[20..24].copy_from_slice(&36u32.to_le_bytes());

    // XSDT Address (bytes 24..32): pointer to our XSDT.
    rsdp[24..32].copy_from_slice(&XSDT_GPA.to_le_bytes());

    // Extended checksum (byte 32): covers all 36 bytes.
    // First zero both checksum fields, compute, then set.
    rsdp[8] = 0;
    rsdp[32] = 0;

    // Legacy checksum (byte 8): covers bytes 0..20 only.
    let legacy_sum: u8 = rsdp[0..20].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    rsdp[8] = (!legacy_sum).wrapping_add(1);

    // Extended checksum (byte 32): covers all 36 bytes.
    let ext_sum: u8 = rsdp.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    rsdp[32] = (!ext_sum).wrapping_add(1);

    rsdp
}

/// Build an ACPI SDT (System Description Table) header.
///
/// `signature`: 4-byte table signature (e.g. "XSDT", "APIC").
/// `total_length`: total table size including header.
fn build_sdt_header(signature: &[u8; 4], total_length: u32) -> [u8; 36] {
    let mut hdr = [0u8; 36];

    // Signature (0..4)
    hdr[0..4].copy_from_slice(signature);
    // Length (4..8)
    hdr[4..8].copy_from_slice(&total_length.to_le_bytes());
    // Revision (8): 1
    hdr[8] = 1;
    // Checksum (9): computed by caller after full table is assembled.
    // OEM ID (10..16)
    hdr[10..16].copy_from_slice(b"HITZ  ");
    // OEM Table ID (16..24)
    hdr[16..24].copy_from_slice(b"HITZVM  ");
    // OEM Revision (24..28)
    hdr[24..28].copy_from_slice(&1u32.to_le_bytes());
    // Creator ID (28..32)
    hdr[28..32].copy_from_slice(b"HITZ");
    // Creator Revision (32..36)
    hdr[32..36].copy_from_slice(&1u32.to_le_bytes());

    hdr
}

/// Build the XSDT (Extended System Description Table).
///
/// Contains a single pointer to the MADT at [`MADT_GPA`].
/// Returns: `(xsdt_bytes, total_length)`.
pub fn build_xsdt() -> Vec<u8> {
    // XSDT = 36-byte header + one 8-byte pointer to MADT
    let total_length: u32 = 36 + 8;
    let mut xsdt = Vec::with_capacity(total_length as usize);

    let header = build_sdt_header(b"XSDT", total_length);
    xsdt.extend_from_slice(&header);

    // Pointer to MADT
    xsdt.extend_from_slice(&MADT_GPA.to_le_bytes());

    // Compute and set checksum at byte 9
    xsdt[9] = acpi_checksum(&xsdt);

    xsdt
}

/// MADT Local APIC entry (type 0), 8 bytes.
struct MadtLapicEntry {
    acpi_processor_id: u8,
    apic_id: u8,
    flags: u32, // bit 0: enabled
}

impl MadtLapicEntry {
    fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = 0; // type = Processor Local APIC
        buf[1] = 8; // length
        buf[2] = self.acpi_processor_id;
        buf[3] = self.apic_id;
        buf[4..8].copy_from_slice(&self.flags.to_le_bytes());
        buf
    }
}

/// Build the MADT (Multiple APIC Description Table) for `cpu_count` CPUs.
///
/// Each CPU gets a Local APIC entry with APIC ID = CPU index.
///
/// # Errors
///
/// Returns `BootError::InvalidBootParams` if `cpu_count` is 0 or > 255.
pub fn build_madt(cpu_count: u32) -> Result<Vec<u8>, BootError> {
    if cpu_count == 0 || cpu_count > 255 {
        return Err(BootError::InvalidBootParams(format!(
            "cpu_count must be 1..=255, got {cpu_count}"
        )));
    }

    // MADT = 36-byte SDT header + 8-byte MADT-specific header + N * 8-byte LAPIC entries
    let madt_header_extra: u32 = 8; // Local APIC Address (4) + Flags (4)
    let lapic_entries_size = cpu_count * 8;
    let total_length = 36 + madt_header_extra + lapic_entries_size;

    let mut madt = Vec::with_capacity(total_length as usize);

    let header = build_sdt_header(b"APIC", total_length);
    madt.extend_from_slice(&header);

    // Local APIC Address (4 bytes): standard 0xFEE00000
    madt.extend_from_slice(&LOCAL_APIC_ADDRESS.to_le_bytes());
    // Flags (4 bytes): bit 0 = PCAT_COMPAT (legacy 8259 present) = 1
    madt.extend_from_slice(&1u32.to_le_bytes());

    // One LAPIC entry per CPU
    for i in 0..cpu_count {
        let entry = MadtLapicEntry {
            acpi_processor_id: i as u8,
            apic_id: i as u8,
            flags: 1, // enabled
        };
        madt.extend_from_slice(&entry.to_bytes());
    }

    // Compute and set checksum at byte 9
    madt[9] = acpi_checksum(&madt);

    Ok(madt)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn verify_checksum(data: &[u8]) -> bool {
        data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b)) == 0
    }

    #[test]
    fn rsdp_checksums_valid() {
        let rsdp = build_rsdp();
        // Legacy checksum covers bytes 0..20
        assert!(
            verify_checksum(&rsdp[0..20]),
            "RSDP legacy checksum failed"
        );
        // Extended checksum covers all 36 bytes
        assert!(verify_checksum(&rsdp), "RSDP extended checksum failed");
    }

    #[test]
    fn rsdp_signature() {
        let rsdp = build_rsdp();
        assert_eq!(&rsdp[0..8], b"RSD PTR ");
    }

    #[test]
    fn rsdp_revision_2() {
        let rsdp = build_rsdp();
        assert_eq!(rsdp[15], 2);
    }

    #[test]
    fn rsdp_xsdt_pointer() {
        let rsdp = build_rsdp();
        let ptr = u64::from_le_bytes(rsdp[24..32].try_into().unwrap());
        assert_eq!(ptr, XSDT_GPA);
    }

    #[test]
    fn xsdt_checksum_valid() {
        let xsdt = build_xsdt();
        assert!(verify_checksum(&xsdt), "XSDT checksum failed");
    }

    #[test]
    fn xsdt_contains_madt_pointer() {
        let xsdt = build_xsdt();
        // MADT pointer is at bytes 36..44 (after the 36-byte header)
        let ptr = u64::from_le_bytes(xsdt[36..44].try_into().unwrap());
        assert_eq!(ptr, MADT_GPA);
    }

    #[test]
    fn madt_1_cpu() {
        let madt = build_madt(1).expect("build madt");
        assert!(verify_checksum(&madt), "MADT checksum failed");
        // Header (36) + MADT extra (8) + 1 LAPIC entry (8) = 52
        assert_eq!(madt.len(), 52);
        // Signature
        assert_eq!(&madt[0..4], b"APIC");
        // LAPIC entry: type=0, len=8, proc_id=0, apic_id=0, flags=1
        assert_eq!(madt[44], 0); // type
        assert_eq!(madt[45], 8); // len
        assert_eq!(madt[46], 0); // proc_id
        assert_eq!(madt[47], 0); // apic_id
    }

    #[test]
    fn madt_4_cpus() {
        let madt = build_madt(4).expect("build madt");
        assert!(verify_checksum(&madt), "MADT checksum failed");
        // Header (36) + MADT extra (8) + 4 LAPIC entries (32) = 76
        assert_eq!(madt.len(), 76);
        // Check 4th LAPIC entry at offset 44 + 3*8 = 68
        assert_eq!(madt[68], 0); // type
        assert_eq!(madt[69], 8); // len
        assert_eq!(madt[70], 3); // proc_id = 3
        assert_eq!(madt[71], 3); // apic_id = 3
    }

    #[test]
    fn madt_0_cpus_fails() {
        assert!(build_madt(0).is_err());
    }

    #[test]
    fn madt_256_cpus_fails() {
        assert!(build_madt(256).is_err());
    }
}
```

### Step 2: Register the module in lib.rs

Modify `crates/hitz-boot/src/lib.rs` — add after `pub mod page_tables;`:

```rust
pub mod acpi;
```

And add to the re-exports:

```rust
pub use acpi::{MADT_GPA, RSDP_GPA, XSDT_GPA, build_madt, build_rsdp, build_xsdt};
```

### Step 3: Verify

Run: `cargo test -p hitz-boot`

Expected: All existing tests pass + 8 new ACPI tests pass.

---

## Task 2: BootParams acpi_rsdp_addr Field

**Files:**
- Modify: `crates/hitz-boot/src/boot_params.rs`

**What we're building:** The Linux `boot_params` struct has `acpi_rsdp_addr` at offset `0x070` (a `u64`). This is currently hidden inside our `_pad0: [u8; 0x1E8]` blob. We need to split `_pad0` to expose this field so `boot_and_run` can set it.

### Step 1: Split _pad0 and add acpi_rsdp_addr

In `crates/hitz-boot/src/boot_params.rs`, replace the `BootParams` struct definition:

**Old:**
```rust
pub struct BootParams {
    _pad0: [u8; 0x1E8],
    /// Number of populated E820 entries (offset 0x1E8).
    pub e820_entries: u8,
```

**New:**
```rust
pub struct BootParams {
    _pad0a: [u8; 0x070],
    /// Physical address of the ACPI RSDP table (offset 0x070).
    /// Set by the bootloader so the kernel can find ACPI tables without
    /// scanning the BIOS memory area. 0 = kernel uses default discovery.
    pub acpi_rsdp_addr: u64,
    _pad0b: [u8; 0x170],
    /// Number of populated E820 entries (offset 0x1E8).
    pub e820_entries: u8,
```

Verification: `0x070 + 8 + 0x170 = 0x1E8`. The total struct size stays 4096.

### Step 2: Update Default impl

**Old:**
```rust
impl Default for BootParams {
    fn default() -> Self {
        Self {
            _pad0: [0; 0x1E8],
            e820_entries: 0,
```

**New:**
```rust
impl Default for BootParams {
    fn default() -> Self {
        Self {
            _pad0a: [0; 0x070],
            acpi_rsdp_addr: 0,
            _pad0b: [0; 0x170],
            e820_entries: 0,
```

### Step 3: Add set_acpi_rsdp helper

Add after `set_initramfs_params`:

```rust
/// Set the ACPI RSDP address in `boot_params`.
///
/// When non-zero, the kernel uses this address instead of scanning
/// the BIOS area for the RSDP signature.
pub fn set_acpi_rsdp(bp: &mut BootParams, rsdp_gpa: u64) {
    bp.acpi_rsdp_addr = rsdp_gpa;
}
```

### Step 4: Add to lib.rs re-exports

In `crates/hitz-boot/src/lib.rs`, add `set_acpi_rsdp` to the `boot_params` re-export line:

```rust
pub use boot_params::{
    BOOT_PARAMS_GPA, BootE820Entry, BootParams, CMDLINE_GPA, E820_RAM, E820_RESERVED,
    KERNEL_LOAD_GPA, SetupHeader, build_boot_params, set_acpi_rsdp, set_initramfs_params,
};
```

### Step 5: Add test

Add to the `tests` module in `boot_params.rs`:

```rust
    #[test]
    fn set_acpi_rsdp_works() {
        let mut bp = BootParams::default();
        assert_eq!(bp.acpi_rsdp_addr, 0);
        set_acpi_rsdp(&mut bp, 0x000E_0000);
        assert_eq!(bp.acpi_rsdp_addr, 0x000E_0000);
    }

    #[test]
    fn acpi_rsdp_addr_offset() {
        // Verify the field is at the correct offset (0x070) within the struct.
        let bp = BootParams::default();
        let base = &bp as *const _ as usize;
        let field = &bp.acpi_rsdp_addr as *const _ as usize;
        assert_eq!(field - base, 0x070, "acpi_rsdp_addr must be at offset 0x070");
    }
```

### Step 6: Verify

Run: `cargo test -p hitz-boot`

Expected: All tests pass. `boot_params_size` still asserts 4096.

---

## Task 3: VmConfig `cpus` Field

**Files:**
- Modify: `crates/hitz-api/src/lib.rs`

**What we're building:** Add `cpus: u32` to `VmConfig` with default 1, plus a constant and serde test.

### Step 1: Add constant and field

In `crates/hitz-api/src/lib.rs`:

After `pub const DEFAULT_RAM_MIB: u32 = 256;` add:

```rust
/// Default number of virtual CPUs.
pub const DEFAULT_CPUS: u32 = 1;
```

Add to `VmConfig` struct after `ram_mib`:

```rust
    /// Number of virtual CPUs (default: 1).
    #[serde(default = "default_cpus")]
    pub cpus: u32,
```

Add the serde default helper function (outside the struct, before the `impl VmConfig` block):

```rust
const fn default_cpus() -> u32 {
    DEFAULT_CPUS
}
```

### Step 2: Update ALL VmConfig construction sites

Every place that constructs a `VmConfig` needs `cpus` added. Search with: `VmConfig {` across the workspace.

Files to update (add `cpus: 1,` or `cpus: DEFAULT_CPUS,` or `cpus: args.cpus,`):

1. `crates/hitz-api/src/lib.rs` — all test configs: add `cpus: 1,` (or `cpus: DEFAULT_CPUS,`)
2. `crates/hitz-cli/src/main.rs` — `run_vm` and `VmCommand::Create`: add `cpus: args.cpus,`
3. `crates/hitz-vmm/src/vm.rs` — test configs: add `cpus: 1,`
4. `crates/hitz-daemon/src/vm_manager.rs` — test `make_config()`: add `cpus: 1,`
5. `crates/hitz-whp/src/tests.rs` — all VmConfig constructors: add `cpus: 1,`

### Step 3: Add serde test

Add to the test module in `crates/hitz-api/src/lib.rs`:

```rust
    #[test]
    fn serde_cpus_default() {
        // When cpus is missing from JSON, it should default to 1.
        let json = r#"{"kernel_path":"vmlinux","initramfs_path":null,"disk_path":null,"ram_mib":256,"cmdline":null,"net":null}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.cpus, DEFAULT_CPUS);
    }

    #[test]
    fn serde_cpus_explicit() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cmdline: None,
            net: None,
            cpus: 4,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.cpus, 4);
    }
```

### Step 4: Verify

Run: `cargo test --workspace`

Expected: All tests pass (all VmConfig sites updated).

---

## Task 4: SharedDevices + Run Loop Refactor

**Files:**
- Modify: `crates/hitz-vmm/src/run_loop.rs`
- Modify: `crates/hitz-vmm/src/lib.rs`
- Modify: `crates/hitz-vmm/src/vm.rs` (call site)

**What we're building:** The run loop currently takes `&mut SerialDevice<W>` and `&mut MmioBus` as separate parameters. For multi-vCPU, these must be behind a `Mutex`. We extract a `SharedDevices<W>` struct and change the signature.

### Step 1: Create SharedDevices and update run_vcpu_loop signature

In `crates/hitz-vmm/src/run_loop.rs`:

Add imports at the top:

```rust
use std::sync::Mutex;
```

Add the `SharedDevices` struct after the `ExitReason` enum:

```rust
/// Devices shared across all vCPU threads.
///
/// Each vCPU locks this only during I/O exits (microseconds per lock).
/// Compute-bound guests experience zero contention.
pub struct SharedDevices<W: Write> {
    /// Serial console (COM1).
    pub serial: SerialDevice<W>,
    /// MMIO bus with virtio transports.
    pub mmio_bus: MmioBus,
}
```

Change the `run_vcpu_loop` signature from:

```rust
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    serial: &mut SerialDevice<W>,
    mmio_bus: &mut MmioBus,
    mem: &dyn GuestMemAccess,
    stop_flag: Option<&AtomicBool>,
) -> Result<ExitReason, HalError> {
```

To:

```rust
pub fn run_vcpu_loop<V: Vcpu, W: Write>(
    vcpu: &mut V,
    devices: &Mutex<SharedDevices<W>>,
    mem: &dyn GuestMemAccess,
    stop_flag: &AtomicBool,
) -> Result<ExitReason, HalError> {
```

### Step 2: Update the loop body

The main changes inside the loop:

1. Replace `if let Some(flag) = stop_flag && flag.load(...)` with `if stop_flag.load(Ordering::Relaxed)`.

2. Device access through lock. The key insight: lock once per I/O exit, not for the entire loop iteration. For the poll_devices call and for handle_io_port, acquire the lock, do the work, drop the lock.

Replace the poll_devices section:

```rust
        // Poll devices for async I/O (e.g. network RX).
        {
            let mut devs = devices.lock().expect("device lock poisoned");
            if let Some(vector) = devs.mmio_bus.poll_devices()
                && vcpu.inject_interrupt(vector).is_err()
            {
                pending_irq = Some(vector);
                vcpu.request_interrupt_window()?;
            }
        }
```

For the `VcpuExit::IoPort` arm:

```rust
            VcpuExit::IoPort(io) => {
                let mut devs = devices.lock().expect("device lock poisoned");
                handle_io_port(vcpu, &mut devs.serial, &io)?;
            }
```

For the `VcpuExit::Mmio` arm, the MMIO bus calls need the lock. Wrap the write/read bus calls:

For MMIO writes:
```rust
                    if mmio.is_write {
                        // ... (decode value from registers as before)
                        let irq = {
                            let mut devs = devices.lock().expect("device lock poisoned");
                            devs.mmio_bus.write(mmio.gpa.as_u64(), &data[..size], mem)
                        };
                        advance_rip(vcpu, instr_len)?;
                        if let Some(vector) = irq {
                            // ... (inject_interrupt as before)
                        }
                    }
```

For MMIO reads:
```rust
                    } else {
                        let size = usize::from(decoded.size);
                        let mut data = [0u8; 8];
                        {
                            let devs = devices.lock().expect("device lock poisoned");
                            devs.mmio_bus.read(mmio.gpa.as_u64(), &mut data[..size]);
                        }
                        // ... (set register + advance RIP as before)
                    }
```

### Step 3: Update lib.rs re-exports

In `crates/hitz-vmm/src/lib.rs`, change:

```rust
pub use run_loop::{ExitReason, run_vcpu_loop};
```

to:

```rust
pub use run_loop::{ExitReason, SharedDevices, run_vcpu_loop};
```

### Step 4: Update the single-vCPU call site in vm.rs

In `crates/hitz-vmm/src/vm.rs`, the call to `run_vcpu_loop` needs updating. This is a temporary compatibility step — Task 5 will do the full multi-vCPU refactor. For now, wrap the existing devices in a Mutex:

Add import:
```rust
use std::sync::Mutex;
use crate::run_loop::SharedDevices;
```

Replace the run_vcpu_loop call (around line 280):

```rust
    // ── 14. Run vCPU loop ──
    let devices = Mutex::new(SharedDevices {
        serial: serial,
        mmio_bus: mmio_bus,
    });

    // Create a local stop flag if the caller didn't provide one.
    let local_stop = AtomicBool::new(false);
    let effective_stop = stop_flag.unwrap_or(&local_stop);

    let exit_reason = run_loop::run_vcpu_loop(
        &mut vcpu,
        &devices,
        &*guest_mem_arc,
        effective_stop,
    )?;
```

### Step 5: Verify

Run: `cargo test --workspace`

Expected: All 164+ tests pass. The refactored single-vCPU path behaves identically.

---

## Task 5: Multi-vCPU Boot Pipeline

**Files:**
- Modify: `crates/hitz-vmm/src/vm.rs`

**What we're building:** The core SMP implementation. `boot_and_run` creates N vCPUs, writes ACPI tables to guest memory, spawns per-vCPU OS threads, and coordinates exits.

### Step 1: Add ACPI table imports

In `crates/hitz-vmm/src/vm.rs`, add to the `hitz_boot` import:

```rust
use hitz_boot::{
    BOOT_PARAMS_GPA, CMDLINE_GPA, RSDP_GPA,
    build_boot_params, build_page_tables, build_rsdp, build_xsdt, build_madt,
    load_elf, load_initramfs, set_acpi_rsdp, set_initramfs_params,
};
```

### Step 2: Add cpus validation to validate_config

In `validate_config`, add after the RAM check:

```rust
    if config.cpus == 0 || config.cpus > 255 {
        return Err(VmError::Config(format!(
            "cpus must be 1..=255, got {}",
            config.cpus
        )));
    }
```

### Step 3: Write ACPI tables to guest memory

After step 7 (writing boot_params) and before step 8 (cmdline), add:

```rust
    // ── 7b. Write ACPI tables for SMP ──
    if config.cpus > 1 {
        let rsdp = build_rsdp();
        guest_mem.write_slice(Gpa::new(RSDP_GPA), &rsdp)?;

        let xsdt = build_xsdt();
        guest_mem.write_slice(Gpa::new(hitz_boot::XSDT_GPA), &xsdt)?;

        let madt = build_madt(config.cpus)?;
        guest_mem.write_slice(Gpa::new(hitz_boot::MADT_GPA), &madt)?;

        set_acpi_rsdp(&mut boot_params, RSDP_GPA);

        // Re-write boot_params with the RSDP pointer set.
        guest_mem.write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)?;
    }
```

### Step 4: Create N vCPUs and configure BSP

Replace the current single-vCPU creation (steps 10-11) with:

```rust
    // ── 10. Create partition + map memory ──
    let partition_cfg = PartitionConfig {
        vcpu_count: config.cpus,
        memory_size: MemSizeMiB::new(u64::from(config.ram_mib)),
    };
    let mut partition = hypervisor.create_partition(&partition_cfg)?;
    guest_mem.map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)?;

    // ── 11. Create vCPUs ──
    let mut vcpus = Vec::with_capacity(config.cpus as usize);
    for i in 0..config.cpus {
        let mut vcpu = partition.create_vcpu(VcpuId::new(i))?;
        if i == 0 {
            // BSP: configure for kernel entry.
            boot_regs::configure_sregs(&mut vcpu, pml4_gpa)?;
            boot_regs::configure_regs(
                &mut vcpu,
                load_result.entry_point,
                Gpa::new(BOOT_PARAMS_GPA),
            )?;
        }
        // APs (i > 0): left in default state. WHP's xAPIC emulation
        // starts them in wait-for-SIPI mode. The BSP will send INIT +
        // STARTUP IPIs via the LAPIC ICR register during Linux SMP boot.
        vcpus.push(vcpu);
    }
```

### Step 5: Multi-threaded run loop with exit coordination

Replace the current step 14 (`run_vcpu_loop` call) with:

```rust
    // ── 14. Run vCPU threads ──
    let devices = Arc::new(Mutex::new(SharedDevices {
        serial,
        mmio_bus,
    }));

    let local_stop = AtomicBool::new(false);
    let effective_stop = stop_flag.unwrap_or(&local_stop);

    if vcpus.len() == 1 {
        // Single vCPU: run on the current thread (no spawn overhead).
        let exit_reason = run_loop::run_vcpu_loop(
            &mut vcpus[0],
            &devices,
            &*guest_mem_arc,
            effective_stop,
        )?;
        drop(net_io_handle);
        return Ok(VmRunResult { exit_reason });
    }

    // Multi-vCPU: spawn a thread per vCPU.
    let first_exit: Arc<Mutex<Option<ExitReason>>> = Arc::new(Mutex::new(None));

    let handles: Vec<_> = vcpus
        .into_iter()
        .enumerate()
        .map(|(idx, mut vcpu)| {
            let devs = devices.clone();
            let mem = guest_mem_arc.clone();
            let first = first_exit.clone();

            std::thread::Builder::new()
                .name(format!("vcpu-{idx}"))
                .spawn(move || {
                    let result = run_loop::run_vcpu_loop(
                        &mut vcpu,
                        &devs,
                        &*mem,
                        effective_stop,
                    );

                    // If this vCPU exited with a terminal reason, record it
                    // and signal all others to stop.
                    match &result {
                        Ok(ExitReason::Halt | ExitReason::Shutdown)
                        | Ok(ExitReason::Unexpected(_))
                        | Err(_) => {
                            let mut first_guard = first.lock().expect("first_exit lock");
                            if first_guard.is_none() {
                                if let Ok(ref reason) = result {
                                    *first_guard = Some(reason.clone());
                                }
                            }
                            effective_stop.store(true, Ordering::Relaxed);
                        }
                        Ok(ExitReason::Canceled) => {
                            // Another vCPU already triggered stop.
                        }
                    }

                    result
                })
                .expect("spawn vcpu thread")
        })
        .collect();

    // Join all threads. Use the first terminal exit reason.
    let mut final_reason = ExitReason::Canceled;
    for handle in handles {
        let _ = handle.join();
    }

    if let Some(reason) = first_exit.lock().expect("first_exit lock").take() {
        final_reason = reason;
    }

    drop(net_io_handle);
    Ok(VmRunResult { exit_reason: final_reason })
```

### Step 6: Make ExitReason Clone

In `crates/hitz-vmm/src/run_loop.rs`, add `Clone` to ExitReason:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
```

### Step 7: Fix the stop_flag lifetime for spawned threads

The `effective_stop` reference has a local lifetime. For `std::thread::spawn`, we need `'static`. Use an `Arc<AtomicBool>`:

In the multi-vCPU path, change to:

```rust
    // Multi-vCPU: need Arc for thread-safe stop flag.
    let shared_stop: Arc<AtomicBool> = if let Some(flag) = stop_flag {
        // Caller provided a flag — wrap in an Arc-like view.
        // We can't Arc an existing reference, so create our own and
        // mirror the caller's flag in the loop poll.
        // Actually, simpler: create our own flag and check both.
        // ...
    }
```

Actually, rethinking this. The cleanest approach: always create an `Arc<AtomicBool>` for multi-vCPU. The run loop already checks it each iteration. If the caller has an external stop flag (Ctrl+C), we check it once before each vCPU iteration by having the BSP thread also poll the caller's flag.

**Simpler approach**: Create an `Arc<AtomicBool>` that threads share. The main thread polls the caller's `stop_flag` and sets the shared one. But that adds a polling thread.

**Simplest approach**: Refactor `boot_and_run` to always take `Arc<AtomicBool>` instead of `Option<&AtomicBool>`:

In `vm.rs`, change the signature:

```rust
pub fn boot_and_run<H: Hypervisor, W: Write + Send>(
    hypervisor: &H,
    config: &VmConfig,
    serial_out: W,
    stop_flag: Arc<AtomicBool>,
) -> Result<VmRunResult, VmError> {
```

And update all callers:
- `crates/hitz-cli/src/main.rs`: already creates `Arc::new(AtomicBool::new(false))`
- `crates/hitz-daemon/src/vm_manager.rs`: creates `Arc::new(AtomicBool::new(false))`
- `crates/hitz-whp/src/tests.rs`: pass `Arc::new(AtomicBool::new(false))`

Then each spawned thread just clones the Arc:

```rust
        .map(|(idx, mut vcpu)| {
            let devs = devices.clone();
            let mem = guest_mem_arc.clone();
            let first = first_exit.clone();
            let stop = stop_flag.clone();

            std::thread::Builder::new()
                .name(format!("vcpu-{idx}"))
                .spawn(move || {
                    let result = run_loop::run_vcpu_loop(
                        &mut vcpu,
                        &devs,
                        &*mem,
                        &stop,
                    );
                    // ... exit coordination as above
                })
```

And `run_vcpu_loop` stays as `stop_flag: &AtomicBool` (receives a reference from the Arc deref).

### Step 8: Update validate_config tests

Add to `vm.rs` tests:

```rust
    #[test]
    fn validate_config_cpus_zero() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.cpus = 0;
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("cpus must be"), "unexpected: {err}");
    }

    #[test]
    fn validate_config_cpus_too_many() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        let mut cfg = valid_config(tmp.path().to_path_buf());
        cfg.cpus = 256;
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("cpus must be"), "unexpected: {err}");
    }
```

### Step 9: Verify

Run: `cargo test --workspace`

Expected: All tests pass. Single-vCPU path unchanged. Multi-vCPU code compiles but is not yet exercised (needs --cpus flag from Task 6 or integration tests from Task 7).

---

## Task 6: CLI `--cpus` Flag

**Files:**
- Modify: `crates/hitz-cli/src/main.rs`

### Step 1: Add --cpus to RunArgs

In the `RunArgs` struct, add after `ram`:

```rust
    /// Number of virtual CPUs.
    #[arg(long, default_value_t = hitz_api::DEFAULT_CPUS)]
    cpus: u32,
```

### Step 2: Add --cpus to VmCreateArgs

In the `VmCreateArgs` struct, add after `ram`:

```rust
    /// Number of virtual CPUs.
    #[arg(long, default_value_t = hitz_api::DEFAULT_CPUS)]
    cpus: u32,
```

### Step 3: Wire cpus into VmConfig construction

In `run_vm`, add `cpus: args.cpus,` to the `VmConfig` construction.

In `VmCommand::Create`, add `cpus: args.cpus,` to the `VmConfig` construction.

### Step 4: Update boot_and_run call site for Arc<AtomicBool>

In `run_vm`, the stop_flag is already `Arc::new(AtomicBool::new(false))`. Change the call:

From: `hitz_vmm::boot_and_run(&hypervisor, &config, serial_out, Some(&stop_flag))`
To: `hitz_vmm::boot_and_run(&hypervisor, &config, serial_out, stop_flag.clone())`

### Step 5: Update daemon's boot_and_run call site

In `crates/hitz-daemon/src/vm_manager.rs`, the stop_flag is also `Arc<AtomicBool>`. Change:

From: `hitz_vmm::boot_and_run(&*hv, &config, serial_buf, Some(&*stop_flag))`
To: `hitz_vmm::boot_and_run(&*hv, &config, serial_buf, stop_flag)`

(The `stop_flag` is already an `Arc<AtomicBool>` from `entry.stop_flag`.)

### Step 6: Update verbose output

In `run_vm`, in the verbose block, add after the RAM line:

```rust
        eprintln!("hitz: CPUs {}", args.cpus);
```

### Step 7: Verify

Run: `cargo build --workspace && cargo run --bin hitz -- run --help`

Expected: Shows `--cpus <CPUS>` option with default 1.

Run: `cargo test --workspace`

Expected: All tests pass.

---

## Task 7: Integration Tests

**Files:**
- Modify: `crates/hitz-whp/src/tests.rs`
- Modify: `crates/hitz-whp/Cargo.toml` (if needed)

### Step 1: Add phase8_smp_2vcpu_hello test

This test creates a 2-vCPU partition using `boot_and_run` with a simple hello-world ELF kernel. The BSP writes "Hello" to serial and halts. APs are in wait-for-SIPI and never activated (no ACPI tables in this test since we use the same Phase 2 test ELF). This validates the multi-vCPU plumbing doesn't crash.

```rust
/// Phase 8: Boot hello ELF with 2 vCPUs. APs stay in wait-for-SIPI since
/// there are no ACPI tables. Validates the multi-thread machinery doesn't crash.
#[test]
#[ignore] // Requires WHP
fn phase8_smp_2vcpu_hello() {
    let kernel_path = build_hello_elf();
    let config = hitz_api::VmConfig {
        kernel_path,
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cmdline: Some("console=ttyS0".into()),
        net: None,
        cpus: 2,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let output = SharedWriter::new();
    let stop = Arc::new(AtomicBool::new(false));
    let result = hitz_vmm::boot_and_run(&hv, &config, output.clone(), stop)
        .expect("boot_and_run should succeed");

    assert_eq!(result.exit_reason, hitz_vmm::ExitReason::Halt);

    let serial_output = output.to_string();
    assert!(
        serial_output.contains("Hello"),
        "expected Hello in serial output, got: {serial_output}"
    );
}
```

### Step 2: Add phase8_smp_linux_boot test

This test boots a real Linux kernel with 2 CPUs and verifies that the kernel discovers both via ACPI MADT. Gated on `HITZ_KERNEL_PATH` env var.

```rust
/// Phase 8: Boot real Linux with 2 CPUs, verify SMP bringup in serial output.
///
/// Requires: `HITZ_KERNEL_PATH` env var pointing to a vmlinux binary.
/// Optional: `HITZ_INITRAMFS_PATH` for an initramfs.
///
/// Run: `HITZ_KERNEL_PATH=/path/to/vmlinux cargo test -p hitz-whp -- --ignored phase8_smp_linux`
#[test]
#[ignore] // Requires WHP + kernel binary
fn phase8_smp_linux_boot() {
    let kernel_path = match std::env::var("HITZ_KERNEL_PATH") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            eprintln!("HITZ_KERNEL_PATH not set, skipping phase8_smp_linux_boot");
            return;
        }
    };
    let initramfs_path = std::env::var("HITZ_INITRAMFS_PATH")
        .ok()
        .map(std::path::PathBuf::from);

    let config = hitz_api::VmConfig {
        kernel_path,
        initramfs_path,
        disk_path: None,
        ram_mib: 256,
        cmdline: Some("console=ttyS0 earlyprintk=serial nokaslr".into()),
        net: None,
        cpus: 2,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let output = SharedWriter::new();
    let stop = Arc::new(AtomicBool::new(false));

    // Give the kernel 10 seconds to boot.
    let stop_clone = stop.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        stop_clone.store(true, Ordering::Relaxed);
    });

    let _result = hitz_vmm::boot_and_run(&hv, &config, output.clone(), stop);

    timer.join().unwrap();

    let serial_output = output.to_string();
    // Linux SMP boot prints something like:
    //   "smp: Brought up 1 node, 2 CPUs"
    // or
    //   "Booting Node 0, Processors: #1"
    assert!(
        serial_output.contains("2 CPUs") || serial_output.contains("Processors"),
        "expected SMP boot messages in serial output.\nGot:\n{serial_output}"
    );
}
```

### Step 3: Import Arc and AtomicBool in tests

Make sure the test file has:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
```

### Step 4: Verify

Run: `cargo test -p hitz-whp -- --ignored --test-threads=1 phase8_smp_2vcpu`

Expected: Test passes (BSP halts, AP stays idle, multi-thread join works).

Run: `cargo test --workspace`

Expected: All non-ignored tests pass.

---

## Files Summary

| File | Action | Task |
|------|--------|------|
| `hitz-boot/src/acpi.rs` | **NEW** | 1 |
| `hitz-boot/src/lib.rs` | Modify (add module + re-exports) | 1, 2 |
| `hitz-boot/src/boot_params.rs` | Modify (split _pad0, add acpi_rsdp_addr) | 2 |
| `hitz-api/src/lib.rs` | Modify (add cpus field + constant + tests) | 3 |
| `hitz-vmm/src/run_loop.rs` | Modify (SharedDevices, new signature, Clone) | 4 |
| `hitz-vmm/src/lib.rs` | Modify (re-export SharedDevices) | 4 |
| `hitz-vmm/src/vm.rs` | Modify (multi-vCPU creation, ACPI, threading) | 4, 5 |
| `hitz-cli/src/main.rs` | Modify (--cpus flag, Arc stop_flag) | 6 |
| `hitz-daemon/src/vm_manager.rs` | Modify (Arc stop_flag call site) | 6 |
| `hitz-whp/src/tests.rs` | Modify (cpus: 1, Phase 8 tests) | 3, 7 |

## Verification

After all tasks:

1. `cargo fmt --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace` — all non-ignored tests pass
4. `cargo test -p hitz-whp -- --ignored --test-threads=1 phase8_smp_2vcpu` — 2-vCPU test passes
5. `cargo run --bin hitz -- run --help` — shows `--cpus`
6. Manual: `cargo run --bin hitz -- run --kernel vmlinux --cpus 2 -v` — Linux discovers 2 CPUs
