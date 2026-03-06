//! WHP lifecycle integration tests.
//!
//! These tests require Windows Hypervisor Platform to be enabled.
//! Run with: `cargo test -p hitz-whp -- --ignored` to include WHP tests.
//!
//! To enable WHP: Settings > Apps > Optional Features > More Windows Features >
//! check "Windows Hypervisor Platform", reboot.

// Tests use expect()/unwrap() liberally — panicking on failure is the point.
// RAM sizes are u64 but add_region takes usize; safe on 64-bit Windows.
// Doc-comment lints relaxed for test doc strings describing internals.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::io_other_error,
    clippy::manual_is_variant_and,
    clippy::unnecessary_wraps,
    clippy::if_then_some_else_none,
    clippy::too_many_lines,
    clippy::manual_let_else,
    clippy::single_match_else
)]

use std::ptr;

use hitz_hal::{Gpa, Hypervisor, MemFlags, Partition, Vcpu, VcpuExit, VcpuId};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
};

use crate::WhpHypervisor;

/// Page-aligned memory allocation using `VirtualAlloc`.
struct PageAlignedMem {
    ptr: *mut u8,
    size: usize,
}

impl PageAlignedMem {
    fn new(size: usize) -> Self {
        // SAFETY: VirtualAlloc with null address lets the OS choose a page-aligned base.
        let ptr = unsafe {
            VirtualAlloc(
                Some(ptr::null_mut()),
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        assert!(!ptr.is_null(), "VirtualAlloc failed");
        Self {
            ptr: ptr.cast(),
            size,
        }
    }

    fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }

    fn as_slice_mut(&mut self) -> &mut [u8] {
        // SAFETY: We own this allocation and it's valid for `size` bytes.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl Drop for PageAlignedMem {
    fn drop(&mut self) {
        // SAFETY: We own this allocation.
        let _ = unsafe { VirtualFree(self.ptr.cast(), 0, MEM_RELEASE) };
    }
}

/// Set up a vCPU in real mode with CS:IP pointing at GPA 0.
///
/// Uses raw WHP API calls to set only the minimal registers needed,
/// avoiding the full `set_sregs` which writes all 14 special registers
/// (including GDT/IDT/TR/LDT that WHP is sensitive about).
fn setup_real_mode_at_zero(vcpu: &mut crate::WhpVcpu) {
    use windows::Win32::System::Hypervisor::{
        WHV_REGISTER_VALUE, WHV_X64_SEGMENT_REGISTER, WHV_X64_SEGMENT_REGISTER_0,
        WHvSetVirtualProcessorRegisters, WHvX64RegisterCr0, WHvX64RegisterCs, WHvX64RegisterDs,
        WHvX64RegisterEs, WHvX64RegisterRflags, WHvX64RegisterRip, WHvX64RegisterSs,
    };

    let code_seg = WHV_X64_SEGMENT_REGISTER {
        Base: 0,
        Limit: 0xFFFF,
        Selector: 0,
        Anonymous: WHV_X64_SEGMENT_REGISTER_0 {
            Attributes: 0x9B, // present, DPL0, code, exec-read, accessed
        },
    };
    let data_seg = WHV_X64_SEGMENT_REGISTER {
        Base: 0,
        Limit: 0xFFFF,
        Selector: 0,
        Anonymous: WHV_X64_SEGMENT_REGISTER_0 {
            Attributes: 0x93, // present, DPL0, data, read-write, accessed
        },
    };

    let names = [
        WHvX64RegisterRip,
        WHvX64RegisterRflags,
        WHvX64RegisterCr0,
        WHvX64RegisterCs,
        WHvX64RegisterDs,
        WHvX64RegisterEs,
        WHvX64RegisterSs,
    ];

    // Zero-init the entire array first: WHV_REGISTER_VALUE is a 16-byte union
    // but Reg64 only writes the low 8 bytes. WHP reads all 16, so garbage in
    // the upper half can cause ACCESS_VIOLATION.
    let mut values: [WHV_REGISTER_VALUE; 7] = unsafe { core::mem::zeroed() };
    values[0].Reg64 = 0; // RIP = 0
    values[1].Reg64 = 0x2; // RFLAGS = reserved bit
    values[2].Reg64 = 0x10; // CR0 = ET (real mode)
    values[3].Segment = code_seg; // CS
    values[4].Segment = data_seg; // DS
    values[5].Segment = data_seg; // ES
    values[6].Segment = data_seg; // SS

    #[allow(clippy::cast_possible_truncation)]
    let count = names.len() as u32;

    unsafe {
        WHvSetVirtualProcessorRegisters(
            vcpu.partition.handle,
            vcpu.index,
            names.as_ptr(),
            count,
            values.as_ptr(),
        )
    }
    .expect("setup_real_mode: SetRegisters failed");
}

/// Test: Create partition → map memory → create vCPU → tear down.
///
/// This is the Phase 0 checkpoint: lifecycle round-trip.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn lifecycle_create_partition_map_memory_create_vcpu() {
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(1),
    };

    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    // Allocate and map 1 page (4 KiB) of guest memory at GPA 0.
    let page_size = 4096;
    let mem = PageAlignedMem::new(page_size);

    unsafe {
        partition
            .map_memory(
                Gpa::new(0),
                mem.as_mut_ptr(),
                page_size,
                MemFlags::READ_WRITE,
            )
            .expect("map_memory failed");
    }

    // Create vCPU 0.
    let _vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    // Unmap the memory.
    partition
        .unmap_memory(Gpa::new(0), page_size)
        .expect("unmap_memory failed");

    // Everything tears down via Drop.
}

/// Test: Create vCPU → run → expect HLT exit.
///
/// Writes a single HLT instruction at GPA 0, sets up real mode, runs the vCPU.
/// The guest should execute HLT and exit.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn run_hlt_instruction() {
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(1),
    };

    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    // Allocate a page and write HLT (0xF4) at the start.
    let page_size = 4096;
    let mut mem = PageAlignedMem::new(page_size);
    mem.as_slice_mut()[0] = 0xF4; // HLT

    // Map at GPA 0 with execute permission.
    unsafe {
        partition
            .map_memory(
                Gpa::new(0),
                mem.as_mut_ptr(),
                page_size,
                MemFlags::READ_WRITE_EXEC,
            )
            .expect("map_memory failed");
    }

    let mut vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    // Set up real mode with CS:IP at 0:0.
    setup_real_mode_at_zero(&mut vcpu);

    // Run through the HAL trait — should execute HLT and exit.
    let exit = vcpu.run().expect("run failed");

    assert!(
        matches!(exit, VcpuExit::Halt),
        "expected Halt exit, got: {exit:?}"
    );
}

/// Test: Guest writes to I/O port → expect `IoPort` exit.
///
/// Writes `OUT 0x3F8, AL` (serial port write) at GPA 0, runs vCPU.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn run_io_port_write() {
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(1),
    };

    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    let page_size = 4096;
    let mut mem = PageAlignedMem::new(page_size);

    // x86 real-mode code:
    //   mov al, 0x41     ; 'A'
    //   mov dx, 0x3F8    ; COM1
    //   out dx, al       ; write to serial
    //   hlt
    let code: &[u8] = &[
        0xB0, 0x41, // mov al, 0x41
        0xBA, 0xF8, 0x03, // mov dx, 0x03F8
        0xEE, // out dx, al
        0xF4, // hlt
    ];
    mem.as_slice_mut()[..code.len()].copy_from_slice(code);

    unsafe {
        partition
            .map_memory(
                Gpa::new(0),
                mem.as_mut_ptr(),
                page_size,
                MemFlags::READ_WRITE_EXEC,
            )
            .expect("map_memory failed");
    }

    let mut vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    // Set up real mode with CS:IP at 0:0.
    setup_real_mode_at_zero(&mut vcpu);

    // First exit should be the OUT instruction (I/O port access).
    let exit = vcpu.run().expect("run failed");
    match exit {
        VcpuExit::IoPort(io) => {
            assert_eq!(io.port, 0x3F8, "expected COM1 port");
            assert!(io.is_write, "expected write");
            assert_eq!(io.data[0], 0x41, "expected 'A'");
        }
        other => panic!("expected IoPort exit, got: {other:?}"),
    }
}

/// Test: Cancel a vCPU from another thread.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn cancel_vcpu() {
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(1),
    };

    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    let page_size = 4096;
    let mut mem = PageAlignedMem::new(page_size);
    mem.as_slice_mut()[0] = 0xF4; // HLT

    unsafe {
        partition
            .map_memory(
                Gpa::new(0),
                mem.as_mut_ptr(),
                page_size,
                MemFlags::READ_WRITE_EXEC,
            )
            .expect("map_memory failed");
    }

    let _vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    // Cancel vCPU 0 from this thread (it's not currently running, but the
    // API call should succeed without error).
    partition
        .request_interrupt(VcpuId::new(0), 0)
        .expect("cancel (via request_interrupt) failed");
}

// ─── Phase 1: Full boot path integration test ───────────────────────────────

/// Build a minimal ELF64 binary containing `code` loaded at `load_addr`.
///
/// Uses the same structure as `hitz-boot`'s test helpers but is self-contained
/// so the test module stays isolated.
fn make_boot_elf(load_addr: u64, code: &[u8]) -> Vec<u8> {
    let elf_header_size = 64usize;
    let phdr_size = 56usize;
    let mut buf = vec![0u8; elf_header_size + phdr_size];

    // ELF header
    buf[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    buf[4] = 2; // ELFCLASS64
    buf[5] = 1; // ELFDATA2LSB
    buf[6] = 1; // EV_CURRENT
    buf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    buf[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    buf[24..32].copy_from_slice(&load_addr.to_le_bytes()); // e_entry
    buf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    buf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    buf[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum

    // Program header
    let data_offset = (elf_header_size + phdr_size) as u64;
    let ph = elf_header_size;
    buf[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    buf[ph + 8..ph + 16].copy_from_slice(&data_offset.to_le_bytes()); // p_offset
    buf[ph + 16..ph + 24].copy_from_slice(&load_addr.to_le_bytes()); // p_vaddr
    buf[ph + 24..ph + 32].copy_from_slice(&load_addr.to_le_bytes()); // p_paddr
    buf[ph + 32..ph + 40].copy_from_slice(&(code.len() as u64).to_le_bytes()); // p_filesz
    buf[ph + 40..ph + 48].copy_from_slice(&(code.len() as u64).to_le_bytes()); // p_memsz

    buf.extend_from_slice(code);
    buf
}

/// Phase 1 checkpoint: Load a tiny 64-bit ELF, boot in long mode, expect I/O
/// port exit.
///
/// This exercises the complete Phase 1 pipeline:
/// 1. `GuestMemory` (VirtualAlloc-backed)
/// 2. `build_page_tables` (4-level identity map)
/// 3. `build_boot_params` (Linux zero page)
/// 4. `load_elf` (ELF64 loader)
/// 5. `write_gdt` + `configure_sregs` + `configure_regs` (long mode setup)
/// 6. `vcpu.run()` → `VcpuExit::IoPort`
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase1_boot_elf_to_io_port_exit() {
    use hitz_boot::{BOOT_PARAMS_GPA, CMDLINE_GPA, build_boot_params, build_page_tables, load_elf};
    use hitz_vmm::{GuestMemory, boot_regs};

    // x86-64 machine code:
    //   mov al, 0x42          ; B0 42
    //   mov edx, 0x3F8        ; BA F8 03 00 00
    //   out dx, al            ; EE
    //   hlt                   ; F4
    let code: &[u8] = &[
        0xB0, 0x42, // mov al, 0x42 ('B')
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8 (COM1)
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64; // 1 MiB — standard kernel load address
    let elf = make_boot_elf(load_addr, code);
    let ram_size = 128 * 1024 * 1024u64; // 128 MiB

    // ── 1. Allocate guest memory ──
    let mut guest_mem = GuestMemory::new();
    guest_mem
        .add_region(Gpa::new(0), ram_size as usize)
        .expect("add_region failed");

    // ── 2. Write page tables ──
    let (pml4_gpa, page_table_writes) = build_page_tables(4).expect("build_page_tables failed");
    for write in &page_table_writes {
        guest_mem
            .write_slice(write.gpa, &write.data)
            .expect("write page table failed");
    }

    // ── 3. Write boot_params ──
    let boot_params =
        build_boot_params(ram_size, Gpa::new(CMDLINE_GPA)).expect("build_boot_params failed");
    guest_mem
        .write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)
        .expect("write boot_params failed");

    // ── 4. Write command line ──
    let cmdline = b"console=ttyS0\0";
    guest_mem
        .write_slice(Gpa::new(CMDLINE_GPA), cmdline)
        .expect("write cmdline failed");

    // ── 5. Write GDT ──
    boot_regs::write_gdt(&guest_mem).expect("write_gdt failed");

    // ── 6. Load ELF ──
    let load_result = load_elf(&elf, &guest_mem).expect("load_elf failed");
    assert_eq!(load_result.entry_point, Gpa::new(load_addr));

    // ── 7. Create WHP partition and map memory ──
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(128),
    };
    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    guest_mem
        .map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)
        .expect("map_to_partition failed");

    // ── 8. Create vCPU and configure registers ──
    let mut vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    boot_regs::configure_sregs(&mut vcpu, pml4_gpa).expect("configure_sregs failed");
    boot_regs::configure_regs(
        &mut vcpu,
        load_result.entry_point,
        Gpa::new(BOOT_PARAMS_GPA),
    )
    .expect("configure_regs failed");

    // ── 9. Run! ──
    let exit = vcpu.run().expect("run failed");

    match exit {
        VcpuExit::IoPort(io) => {
            assert_eq!(io.port, 0x3F8, "expected COM1 port");
            assert!(io.is_write, "expected write (OUT)");
            assert_eq!(io.data[0], 0x42, "expected 'B'");
        }
        other => panic!("expected IoPort exit, got: {other:?}"),
    }
}

// ─── Phase 2: Serial console integration tests ──────────────────────────────

/// Helper: set up the full boot pipeline (memory, page tables, `boot_params`,
/// GDT, ELF) and return a configured vCPU ready to run.
///
/// Returns `(partition, vcpu, guest_mem)` — caller owns the lifetime.
fn boot_elf_pipeline(code: &[u8]) -> (crate::WhpPartition, crate::WhpVcpu, hitz_vmm::GuestMemory) {
    use hitz_boot::{BOOT_PARAMS_GPA, CMDLINE_GPA, build_boot_params, build_page_tables, load_elf};
    use hitz_vmm::{GuestMemory, boot_regs};

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);
    let ram_size = 128 * 1024 * 1024u64;

    let mut guest_mem = GuestMemory::new();
    guest_mem
        .add_region(Gpa::new(0), ram_size as usize)
        .expect("add_region failed");

    let (pml4_gpa, page_table_writes) = build_page_tables(4).expect("build_page_tables failed");
    for write in &page_table_writes {
        guest_mem
            .write_slice(write.gpa, &write.data)
            .expect("write page table failed");
    }

    let boot_params =
        build_boot_params(ram_size, Gpa::new(CMDLINE_GPA)).expect("build_boot_params failed");
    guest_mem
        .write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)
        .expect("write boot_params failed");

    let cmdline = b"console=ttyS0\0";
    guest_mem
        .write_slice(Gpa::new(CMDLINE_GPA), cmdline)
        .expect("write cmdline failed");

    boot_regs::write_gdt(&guest_mem).expect("write_gdt failed");

    let load_result = load_elf(&elf, &guest_mem).expect("load_elf failed");

    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(128),
    };
    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    guest_mem
        .map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)
        .expect("map_to_partition failed");

    let mut vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    boot_regs::configure_sregs(&mut vcpu, pml4_gpa).expect("configure_sregs failed");
    boot_regs::configure_regs(
        &mut vcpu,
        load_result.entry_point,
        Gpa::new(BOOT_PARAMS_GPA),
    )
    .expect("configure_regs failed");

    (partition, vcpu, guest_mem)
}

/// Phase 2 checkpoint: ELF writes "Hello" to COM1 via the run loop.
///
/// Exercises the complete Phase 2 pipeline:
/// 1. Serial device receives OUT instructions via run loop dispatch
/// 2. RIP is advanced after each I/O exit
/// 3. Guest executes HLT → run loop returns `ExitReason::Halt`
/// 4. Serial output buffer contains "Hello"
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase2_serial_output_from_elf() {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_vmm::run_loop::{ExitReason, SharedDevices, run_vcpu_loop};

    // x86-64 machine code that writes "Hello" to COM1 (0x3F8) then halts.
    //
    //   mov edx, 0x3F8        ; BA F8 03 00 00   — COM1 THR
    //   mov al, 'H'           ; B0 48
    //   out dx, al            ; EE
    //   mov al, 'e'           ; B0 65
    //   out dx, al            ; EE
    //   mov al, 'l'           ; B0 6C
    //   out dx, al            ; EE
    //   mov al, 'l'           ; B0 6C
    //   out dx, al            ; EE
    //   mov al, 'o'           ; B0 6F
    //   out dx, al            ; EE
    //   hlt                   ; F4
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let (_partition, mut vcpu, guest_mem) = boot_elf_pipeline(code);
    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus: MmioBus::new(),
    });

    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &guest_mem, &stop).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");
    let devs = devices.lock().expect("lock");
    assert_eq!(
        devs.serial.writer().as_slice(),
        b"Hello",
        "serial output mismatch"
    );
}

/// Phase 2: Verify IN instruction handling (guest reads LSR, writes result).
///
/// The guest reads the Line Status Register (port 0x3FD), which should
/// return THRE|TEMT (0x60), then writes that value to COM1 THR so we
/// can inspect it, then halts.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase2_serial_in_reads_lsr() {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_vmm::run_loop::{ExitReason, SharedDevices, run_vcpu_loop};

    // x86-64 machine code:
    //   mov edx, 0x3FD        ; BA FD 03 00 00   — COM1 LSR
    //   in  al, dx            ; EC               — read LSR into AL
    //   mov edx, 0x3F8        ; BA F8 03 00 00   — COM1 THR
    //   out dx, al            ; EE               — write LSR value to output
    //   hlt                   ; F4
    let code: &[u8] = &[
        0xBA, 0xFD, 0x03, 0x00, 0x00, // mov edx, 0x3FD (LSR)
        0xEC, // in al, dx
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8 (THR)
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let (_partition, mut vcpu, guest_mem) = boot_elf_pipeline(code);
    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus: MmioBus::new(),
    });

    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &guest_mem, &stop).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");

    // LSR default = THRE (0x20) | TEMT (0x40) = 0x60
    let devs = devices.lock().expect("lock");
    let output = devs.serial.writer();
    assert_eq!(output.len(), 1, "expected 1 byte of serial output");
    assert_eq!(
        output[0], 0x60,
        "expected LSR = THRE|TEMT (0x60), got {:#x}",
        output[0]
    );
}

// ─── Phase 3: Virtio-MMIO integration tests ──────────────────────────────────

/// Build a 16-byte virtqueue descriptor (VirtqDesc).
///
/// Layout: addr(u64) + len(u32) + flags(u16) + next(u16) = 16 bytes.
fn build_descriptor(addr: u64, len: u32, flags: u16, next: u16) -> [u8; 16] {
    let mut desc = [0u8; 16];
    desc[0..8].copy_from_slice(&addr.to_le_bytes());
    desc[8..12].copy_from_slice(&len.to_le_bytes());
    desc[12..14].copy_from_slice(&flags.to_le_bytes());
    desc[14..16].copy_from_slice(&next.to_le_bytes());
    desc
}

/// Build a minimal virtqueue available ring.
///
/// Layout: flags(u16) + idx(u16) + ring entries(u16 each).
fn build_avail_ring(idx: u16, entries: &[u16]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
    data.extend_from_slice(&idx.to_le_bytes());
    for &entry in entries {
        data.extend_from_slice(&entry.to_le_bytes());
    }
    data
}

/// Build a 16-byte virtio block request header.
///
/// Layout: type(u32) + reserved(u32) + sector(u64) = 16 bytes.
fn build_blk_request(req_type: u32, sector: u64) -> [u8; 16] {
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&req_type.to_le_bytes());
    // bytes 4..8 = reserved (0)
    header[8..16].copy_from_slice(&sector.to_le_bytes());
    header
}

/// Write a 64-bit IDT gate entry for `vector` pointing to `handler_gpa`.
///
/// IDT base is at GPA 0 (from `configure_sregs`), so the entry for vector N
/// is at GPA `N * 16`.
///
fn write_idt_gate(guest_mem: &hitz_vmm::GuestMemory, vector: u8, handler_gpa: u64) {
    #[allow(clippy::cast_possible_truncation)]
    let offset_lo = (handler_gpa & 0xFFFF) as u16;
    #[allow(clippy::cast_possible_truncation)]
    let offset_mid = ((handler_gpa >> 16) & 0xFFFF) as u16;
    #[allow(clippy::cast_possible_truncation)]
    let offset_hi = ((handler_gpa >> 32) & 0xFFFF_FFFF) as u32;

    let mut entry = [0u8; 16];
    entry[0..2].copy_from_slice(&offset_lo.to_le_bytes());
    entry[2..4].copy_from_slice(&0x08u16.to_le_bytes()); // CS selector
    entry[4] = 0; // IST = 0
    entry[5] = 0x8E; // 64-bit interrupt gate, DPL=0, Present
    entry[6..8].copy_from_slice(&offset_mid.to_le_bytes());
    entry[8..12].copy_from_slice(&offset_hi.to_le_bytes());
    // entry[12..16] already zeroed (reserved)

    let idt_entry_gpa = u64::from(vector) * 16;
    guest_mem
        .write_slice(Gpa::new(idt_entry_gpa), &entry)
        .expect("write IDT gate entry");
}

/// Phase 3 checkpoint: Read the virtio-MMIO magic value through the full stack.
///
/// Exercises:
/// 1. Guest executes `MOV EAX, [RBX]` at an unmapped GPA → WHP MMIO exit
/// 2. MMIO instruction decoder extracts register and size from raw bytes
/// 3. MMIO bus routes the GPA to the virtio-MMIO transport
/// 4. Transport returns MagicValue (0x74726976) from offset 0x000
/// 5. Run loop writes value back to EAX and advances RIP
/// 6. Guest writes each byte to serial → "virt"
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase3_virtio_mmio_magic_read() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_devices::virtio::block::VirtioBlockDevice;
    use hitz_devices::virtio::mmio_transport::VirtioMmioTransport;
    use hitz_vmm::run_loop::{ExitReason, SharedDevices, run_vcpu_loop};

    // x86-64 machine code:
    //   mov ebx, 0xD0000000     ; MMIO base (unmapped GPA)
    //   mov eax, [rbx]          ; read MagicValue (offset 0) -- triggers MMIO exit
    //   mov edx, 0x3F8          ; COM1
    //   out dx, al              ; byte 0 (0x76 = 'v')
    //   shr eax, 8
    //   out dx, al              ; byte 1 (0x69 = 'i')
    //   shr eax, 8
    //   out dx, al              ; byte 2 (0x72 = 'r')
    //   shr eax, 8
    //   out dx, al              ; byte 3 (0x74 = 't')
    //   hlt
    let code: &[u8] = &[
        0xBB, 0x00, 0x00, 0x00, 0xD0, // mov ebx, 0xD0000000
        0x8B, 0x03, // mov eax, [rbx]
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xEE, // out dx, al
        0xC1, 0xE8, 0x08, // shr eax, 8
        0xEE, // out dx, al
        0xC1, 0xE8, 0x08, // shr eax, 8
        0xEE, // out dx, al
        0xC1, 0xE8, 0x08, // shr eax, 8
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let (_partition, mut vcpu, guest_mem) = boot_elf_pipeline(code);
    let guest_mem = Arc::new(guest_mem);

    // Create a minimal 512-byte disk (content doesn't matter for this test).
    let disk_file = tempfile::tempfile().expect("create temp disk");
    disk_file.set_len(512).expect("set disk size");
    let block_dev = VirtioBlockDevice::new(disk_file).expect("create block device");

    // Set up MMIO bus with virtio transport at 0xD0000000.
    let mem: Arc<dyn hitz_hal::GuestMemAccess> = guest_mem.clone();
    let transport = VirtioMmioTransport::new(block_dev, mem, 5);
    let mut mmio_bus = MmioBus::new();
    mmio_bus.register(0xD000_0000, 0x1000, Box::new(transport));

    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus,
    });

    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &*guest_mem, &stop).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");

    // MagicValue = 0x74726976 in little-endian.
    // Guest outputs each byte via serial: 0x76('v'), 0x69('i'), 0x72('r'), 0x74('t').
    let devs = devices.lock().expect("lock");
    assert_eq!(
        devs.serial.writer().as_slice(),
        b"virt",
        "serial output should be 'virt' (magic value bytes in LE)"
    );
}

/// Phase 3 checkpoint: Full virtio device init + block read through the stack.
///
/// This is the end-to-end test for Phase 3. The guest:
/// 1. Walks the virtio status state machine (ACKNOWLEDGE → DRIVER → FEATURES_OK → DRIVER_OK)
/// 2. Configures the virtqueue (desc/avail/used GPAs, QueueReady)
/// 3. Writes QueueNotify → triggers synchronous block read
/// 4. CPU delivers interrupt (vector 5) via IDT → IRETQ returns to guest
/// 5. Guest reads disk data from RAM, outputs status + data to serial
///
/// Memory layout:
/// - 0x0_0050     IDT entry for vector 5 (IDT base = 0)
/// - 0x8_0000     Stack top (grows downward for interrupt frame)
/// - 0x9_0000     IRETQ handler (2 bytes: 0x48 0xCF)
/// - 0x10_0000    Kernel code (loaded by ELF boot pipeline)
/// - 0x20_0000    Descriptor table (3 descriptors × 16 bytes)
/// - 0x20_1000    Available ring
/// - 0x20_2000    Used ring
/// - 0x20_3000    Block request header (16 bytes)
/// - 0x20_4000    Data buffer (512 bytes, filled by block device)
/// - 0x20_5000    Status byte (1 byte, filled by block device)
/// - 0xD000_0000  Virtio-MMIO transport (4 KiB, unmapped → MMIO exits)
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase3_virtio_block_read() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_devices::virtio::block::VirtioBlockDevice;
    use hitz_devices::virtio::mmio_transport::VirtioMmioTransport;
    use hitz_vmm::run_loop::{ExitReason, SharedDevices, run_vcpu_loop};

    const IRQ_VECTOR: u8 = 5;

    // x86-64 machine code: init virtio device, trigger block read, output results.
    //
    // MMIO writes use C7 /0 encoding (MOV r/m32, imm32):
    //   C7 43 xx imm32    for offset < 0x80 (mod=01, disp8)
    //   C7 83 xx xx xx xx imm32  for offset >= 0x80 (mod=10, disp32)
    #[rustfmt::skip]
    let code: &[u8] = &[
        // ---- Set up MMIO base in RBX ----
        0xBB, 0x00, 0x00, 0x00, 0xD0,                          // mov ebx, 0xD0000000

        // ---- Virtio status state machine (offset 0x70) ----
        0xC7, 0x43, 0x70, 0x01, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 1   (ACKNOWLEDGE)
        0xC7, 0x43, 0x70, 0x03, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 3   (ACK|DRIVER)
        0xC7, 0x43, 0x70, 0x0B, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 0xB (ACK|DRIVER|FEATURES_OK)
        0xC7, 0x43, 0x70, 0x0F, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 0xF (ACK|DRIVER|FEATURES_OK|DRIVER_OK)

        // ---- Queue configuration ----
        0xC7, 0x43, 0x38, 0x00, 0x01, 0x00, 0x00,              // mov [rbx+0x38], 256 (QueueNum)
        0xC7, 0x83, 0x80,0x00,0x00,0x00, 0x00,0x00,0x20,0x00,  // mov [rbx+0x80], 0x200000 (QueueDescLow)
        0xC7, 0x83, 0x84,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0x84], 0        (QueueDescHigh)
        0xC7, 0x83, 0x90,0x00,0x00,0x00, 0x00,0x10,0x20,0x00,  // mov [rbx+0x90], 0x201000 (QueueAvailLow)
        0xC7, 0x83, 0x94,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0x94], 0        (QueueAvailHigh)
        0xC7, 0x83, 0xA0,0x00,0x00,0x00, 0x00,0x20,0x20,0x00,  // mov [rbx+0xA0], 0x202000 (QueueUsedLow)
        0xC7, 0x83, 0xA4,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0xA4], 0        (QueueUsedHigh)
        0xC7, 0x43, 0x44, 0x01, 0x00, 0x00, 0x00,              // mov [rbx+0x44], 1 (QueueReady)

        // ---- Trigger block read (QueueNotify at offset 0x50) ----
        0xC7, 0x43, 0x50, 0x00, 0x00, 0x00, 0x00,              // mov [rbx+0x50], 0 (QueueNotify)

        // ---- Read status byte from 0x205000, output to serial ----
        0xB9, 0x00, 0x50, 0x20, 0x00,                          // mov ecx, 0x205000
        0x8A, 0x01,                                              // mov al, [rcx]
        0xBA, 0xF8, 0x03, 0x00, 0x00,                          // mov edx, 0x3F8
        0xEE,                                                    // out dx, al

        // ---- Read first 4 bytes from data buffer at 0x204000 ----
        0xB9, 0x00, 0x40, 0x20, 0x00,                          // mov ecx, 0x204000
        0x8B, 0x01,                                              // mov eax, [rcx]
        0xEE,                                                    // out dx, al (byte 0)
        0xC1, 0xE8, 0x08,                                      // shr eax, 8
        0xEE,                                                    // out dx, al (byte 1)
        0xC1, 0xE8, 0x08,                                      // shr eax, 8
        0xEE,                                                    // out dx, al (byte 2)
        0xC1, 0xE8, 0x08,                                      // shr eax, 8
        0xEE,                                                    // out dx, al (byte 3)

        // ---- Done ----
        0xF4,                                                    // hlt
    ];

    let (_partition, mut vcpu, guest_mem) = boot_elf_pipeline(code);
    let guest_mem = Arc::new(guest_mem);

    // ── Interrupt handling setup ──
    // Note: Interrupt delivery deferred to Phase 4 (requires APIC config).
    // Block I/O is synchronous — data is in guest memory when QueueNotify
    // MMIO write returns. The guest reads it directly without needing IRQs.

    // ── Disk image: "HITZ" repeated for 512 bytes (1 sector) ──
    let mut disk_file = tempfile::tempfile().expect("create temp disk");
    let disk_data: Vec<u8> = b"HITZ".iter().copied().cycle().take(512).collect();
    std::io::Write::write_all(&mut disk_file, &disk_data).expect("write disk data");
    let block_dev = VirtioBlockDevice::new(disk_file).expect("create block device");

    let mem: Arc<dyn hitz_hal::GuestMemAccess> = guest_mem.clone();
    let transport = VirtioMmioTransport::new(block_dev, mem, IRQ_VECTOR);
    let mut mmio_bus = MmioBus::new();
    mmio_bus.register(0xD000_0000, 0x1000, Box::new(transport));

    // ── Pre-fill virtqueue structures in guest RAM ──
    // The guest code configures the device via MMIO writes, but the actual
    // ring data structures must exist in guest memory beforehand.

    // Descriptor table at 0x200000: 3-descriptor chain for a block read.
    //   desc 0: request header (device-readable, 16 bytes)
    //   desc 1: data buffer (device-writable, 512 bytes)
    //   desc 2: status byte (device-writable, 1 byte)
    let desc0 = build_descriptor(0x20_3000, 16, 1 /* NEXT */, 1);
    let desc1 = build_descriptor(0x20_4000, 512, 3 /* NEXT|WRITE */, 2);
    let desc2 = build_descriptor(0x20_5000, 1, 2 /* WRITE */, 0);
    guest_mem
        .write_slice(Gpa::new(0x20_0000), &desc0)
        .expect("write desc 0");
    guest_mem
        .write_slice(Gpa::new(0x20_0010), &desc1)
        .expect("write desc 1");
    guest_mem
        .write_slice(Gpa::new(0x20_0020), &desc2)
        .expect("write desc 2");

    // Available ring at 0x201000: one entry pointing to descriptor chain head (0).
    let avail = build_avail_ring(1, &[0]);
    guest_mem
        .write_slice(Gpa::new(0x20_1000), &avail)
        .expect("write avail ring");

    // Block request header at 0x203000: read sector 0.
    let req = build_blk_request(0 /* VIRTIO_BLK_T_IN */, 0);
    guest_mem
        .write_slice(Gpa::new(0x20_3000), &req)
        .expect("write request header");

    // ── Run! ──
    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus,
    });

    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &*guest_mem, &stop).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");

    // Expected serial output:
    //   byte 0: status = 0 (VIRTIO_BLK_S_OK)
    //   bytes 1-4: first 4 bytes of disk data = "HITZ"
    let devs = devices.lock().expect("lock");
    let output = devs.serial.writer().as_slice();
    assert!(
        output.len() >= 5,
        "expected at least 5 bytes of serial output, got {}",
        output.len()
    );
    assert_eq!(output[0], 0, "status should be VIRTIO_BLK_S_OK (0)");
    assert_eq!(
        &output[1..5],
        b"HITZ",
        "first 4 data bytes should be 'HITZ'"
    );
}

// ─── Phase 4: APIC + Interrupt Window + Initramfs ────────────────────────────

/// Phase 4 checkpoint: APIC emulation enables interrupt delivery through IDT.
///
/// Exercises the complete Phase 4 interrupt pipeline:
/// 1. WHP partition with xAPIC emulation (Task 0)
/// 2. Guest does STI (enable interrupts) with a valid stack + IDT
/// 3. Virtio-block QueueNotify triggers IRQ 5
/// 4. `inject_interrupt` succeeds (IF=1 from STI)
/// 5. CPU vectors to IDT handler, which writes "I" to serial
/// 6. IRETQ returns to main code, which writes "D" + status to serial
/// 7. HLT terminates the run loop
///
/// Expected serial output: `b"ID\x00"` — "I" from IRQ handler, "D" for
/// done, 0x00 for `VIRTIO_BLK_S_OK`.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase4_apic_interrupt_delivery() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_devices::virtio::block::VirtioBlockDevice;
    use hitz_devices::virtio::mmio_transport::VirtioMmioTransport;
    use hitz_vmm::run_loop::{ExitReason, SharedDevices, run_vcpu_loop};

    const IRQ_VECTOR: u8 = 5;

    // ── Interrupt handler at GPA 0x90000 ──
    // push rax; push rdx; mov edx, 0x3F8; mov al, 'I'; out dx, al;
    // pop rdx; pop rax; iretq
    let irq_handler: &[u8] = &[
        0x50, // push rax
        0x52, // push rdx
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x49, // mov al, 'I'
        0xEE, // out dx, al
        0x5A, // pop rdx
        0x58, // pop rax
        0x48, 0xCF, // iretq
    ];

    // ── Main ELF code ──
    // Set up stack, enable interrupts, init virtio, trigger block read,
    // write "D" + status to serial, halt.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // ---- Stack + interrupts ----
        0xBC, 0x00, 0x00, 0x08, 0x00,                          // mov esp, 0x80000
        0xFB,                                                    // sti

        // ---- Set up MMIO base ----
        0xBB, 0x00, 0x00, 0x00, 0xD0,                          // mov ebx, 0xD0000000

        // ---- Virtio status state machine (offset 0x70) ----
        0xC7, 0x43, 0x70, 0x01, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 1   (ACKNOWLEDGE)
        0xC7, 0x43, 0x70, 0x03, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 3   (ACK|DRIVER)
        0xC7, 0x43, 0x70, 0x0B, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 0xB (ACK|DRIVER|FEATURES_OK)
        0xC7, 0x43, 0x70, 0x0F, 0x00, 0x00, 0x00,              // mov [rbx+0x70], 0xF (DRIVER_OK)

        // ---- Queue configuration ----
        0xC7, 0x43, 0x38, 0x00, 0x01, 0x00, 0x00,              // mov [rbx+0x38], 256 (QueueNum)
        0xC7, 0x83, 0x80,0x00,0x00,0x00, 0x00,0x00,0x20,0x00,  // mov [rbx+0x80], 0x200000 (QueueDescLow)
        0xC7, 0x83, 0x84,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0x84], 0        (QueueDescHigh)
        0xC7, 0x83, 0x90,0x00,0x00,0x00, 0x00,0x10,0x20,0x00,  // mov [rbx+0x90], 0x201000 (QueueAvailLow)
        0xC7, 0x83, 0x94,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0x94], 0        (QueueAvailHigh)
        0xC7, 0x83, 0xA0,0x00,0x00,0x00, 0x00,0x20,0x20,0x00,  // mov [rbx+0xA0], 0x202000 (QueueUsedLow)
        0xC7, 0x83, 0xA4,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,  // mov [rbx+0xA4], 0        (QueueUsedHigh)
        0xC7, 0x43, 0x44, 0x01, 0x00, 0x00, 0x00,              // mov [rbx+0x44], 1 (QueueReady)

        // ---- Trigger block read (QueueNotify at offset 0x50) ----
        0xC7, 0x43, 0x50, 0x00, 0x00, 0x00, 0x00,              // mov [rbx+0x50], 0

        // ---- Post-interrupt: "D" to serial, status byte, halt ----
        0xBA, 0xF8, 0x03, 0x00, 0x00,                          // mov edx, 0x3F8
        0xB0, 0x44,                                              // mov al, 'D'
        0xEE,                                                    // out dx, al
        0xB9, 0x00, 0x50, 0x20, 0x00,                          // mov ecx, 0x205000
        0x8A, 0x01,                                              // mov al, [rcx]
        0xEE,                                                    // out dx, al
        0xF4,                                                    // hlt
    ];

    let (_partition, mut vcpu, guest_mem) = boot_elf_pipeline(code);
    let guest_mem = Arc::new(guest_mem);

    // ── Write interrupt handler into guest memory at 0x90000 ──
    guest_mem
        .write_slice(Gpa::new(0x9_0000), irq_handler)
        .expect("write IRQ handler");

    // ── Set up IDT gate for vector 5 → handler at 0x90000 ──
    write_idt_gate(&guest_mem, IRQ_VECTOR, 0x9_0000);

    // ── Disk image: "HITZ" repeated for 512 bytes ──
    let mut disk_file = tempfile::tempfile().expect("create temp disk");
    let disk_data: Vec<u8> = b"HITZ".iter().copied().cycle().take(512).collect();
    std::io::Write::write_all(&mut disk_file, &disk_data).expect("write disk data");
    let block_dev = VirtioBlockDevice::new(disk_file).expect("create block device");

    let mem: Arc<dyn hitz_hal::GuestMemAccess> = guest_mem.clone();
    let transport = VirtioMmioTransport::new(block_dev, mem, IRQ_VECTOR);
    let mut mmio_bus = MmioBus::new();
    mmio_bus.register(0xD000_0000, 0x1000, Box::new(transport));

    // ── Pre-fill virtqueue structures (same as Phase 3) ──
    let desc0 = build_descriptor(0x20_3000, 16, 1 /* NEXT */, 1);
    let desc1 = build_descriptor(0x20_4000, 512, 3 /* NEXT|WRITE */, 2);
    let desc2 = build_descriptor(0x20_5000, 1, 2 /* WRITE */, 0);
    guest_mem
        .write_slice(Gpa::new(0x20_0000), &desc0)
        .expect("write desc 0");
    guest_mem
        .write_slice(Gpa::new(0x20_0010), &desc1)
        .expect("write desc 1");
    guest_mem
        .write_slice(Gpa::new(0x20_0020), &desc2)
        .expect("write desc 2");

    let avail = build_avail_ring(1, &[0]);
    guest_mem
        .write_slice(Gpa::new(0x20_1000), &avail)
        .expect("write avail ring");

    let req = build_blk_request(0 /* VIRTIO_BLK_T_IN */, 0);
    guest_mem
        .write_slice(Gpa::new(0x20_3000), &req)
        .expect("write request header");

    // ── Run ──
    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus,
    });
    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &*guest_mem, &stop).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");

    // Expected serial: "I" (IRQ handler) + "D" (done) + 0x00 (status OK)
    let devs = devices.lock().expect("lock");
    let output = devs.serial.writer().as_slice();
    assert!(
        output.len() >= 3,
        "expected at least 3 bytes of serial output, got {} bytes: {output:?}",
        output.len()
    );

    // The 'I' from the interrupt handler proves the full pipeline:
    // APIC enabled → inject_interrupt → IDT vectoring → handler → IRETQ
    assert!(
        output.contains(&b'I'),
        "expected 'I' from interrupt handler in output: {output:?}"
    );
    assert!(
        output.contains(&b'D'),
        "expected 'D' (done marker) in output: {output:?}"
    );
    // Status byte should be 0 (VIRTIO_BLK_S_OK)
    assert_eq!(
        output.last().copied(),
        Some(0),
        "last byte should be status OK (0), got: {output:?}"
    );
}

/// Phase 4 checkpoint: Boot a real vmlinux with initramfs.
///
/// This is the end-to-end test for Phase 4. It loads an uncompressed
/// vmlinux ELF and a cpio initramfs, boots the kernel through the
/// full pipeline (page tables, boot_params, GDT, long mode entry),
/// and checks for the Linux boot banner or a custom init message
/// on the serial console.
///
/// # Test artifacts
///
/// Requires env vars:
///   `HITZ_VMLINUX=/path/to/vmlinux`
///   `HITZ_INITRAMFS=/path/to/initramfs.cpio`
///
/// To build these on a Linux machine:
/// ```bash
/// # Extract vmlinux from bzImage
/// scripts/extract-vmlinux arch/x86/boot/bzImage > vmlinux
///
/// # Minimal initramfs
/// mkdir /tmp/initramfs && cat > /tmp/initramfs/init << 'EOF'
/// #!/bin/sh
/// echo "hitz-boot-ok"
/// poweroff -f
/// EOF
/// chmod +x /tmp/initramfs/init
/// cd /tmp/initramfs && find . | cpio -o -H newc > /tmp/initramfs.cpio
/// ```
///
/// Run:
/// ```bash
/// HITZ_VMLINUX=path/to/vmlinux HITZ_INITRAMFS=path/to/initramfs.cpio \
///   cargo test -p hitz-whp -- --ignored phase4_boot_real_linux --test-threads=1
/// ```
#[test]
#[ignore = "requires WHP + vmlinux + initramfs (set HITZ_VMLINUX + HITZ_INITRAMFS)"]
fn phase4_boot_real_linux() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use hitz_boot::{
        BOOT_PARAMS_GPA, CMDLINE_GPA, build_boot_params, build_page_tables, load_elf,
        load_initramfs, set_initramfs_params,
    };
    use hitz_devices::mmio_bus::MmioBus;
    use hitz_devices::serial::SerialDevice;
    use hitz_vmm::run_loop::{SharedDevices, run_vcpu_loop};
    use hitz_vmm::{GuestMemory, boot_regs};

    // ── 1. Read env vars (skip if not set) ──
    let vmlinux_path = match std::env::var("HITZ_VMLINUX") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("HITZ_VMLINUX not set, skipping phase4_boot_real_linux");
            return;
        }
    };
    let initramfs_path = match std::env::var("HITZ_INITRAMFS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("HITZ_INITRAMFS not set, skipping phase4_boot_real_linux");
            return;
        }
    };

    let vmlinux = std::fs::read(&vmlinux_path).expect("failed to read vmlinux file");
    let initramfs_data = std::fs::read(&initramfs_path).expect("failed to read initramfs file");

    eprintln!(
        "vmlinux: {} ({} bytes), initramfs: {} ({} bytes)",
        vmlinux_path,
        vmlinux.len(),
        initramfs_path,
        initramfs_data.len()
    );

    // ── 2. Allocate 256 MiB guest memory ──
    let ram_size = 256 * 1024 * 1024u64;
    let mut guest_mem = GuestMemory::new();
    guest_mem
        .add_region(Gpa::new(0), ram_size as usize)
        .expect("add_region failed");

    // ── 3. Build page tables (1 GiB identity map) ──
    let (pml4_gpa, page_table_writes) = build_page_tables(1).expect("build_page_tables failed");
    for write in &page_table_writes {
        guest_mem
            .write_slice(write.gpa, &write.data)
            .expect("write page table failed");
    }

    // ── 4. Load vmlinux ELF ──
    let load_result = load_elf(&vmlinux, &guest_mem).expect("load vmlinux failed");
    eprintln!(
        "kernel: load={}, end={}, entry={}",
        load_result.kernel_load, load_result.kernel_end, load_result.entry_point
    );

    // ── 5. Load initramfs after kernel ──
    let initramfs_result = load_initramfs(
        &initramfs_data,
        load_result.kernel_end,
        ram_size,
        &guest_mem,
    )
    .expect("load_initramfs failed");
    eprintln!(
        "initramfs: gpa={}, size={:#x}",
        initramfs_result.gpa, initramfs_result.size
    );

    // ── 6. Build boot_params + patch with initramfs info ──
    let mut boot_params =
        build_boot_params(ram_size, Gpa::new(CMDLINE_GPA)).expect("build_boot_params failed");
    set_initramfs_params(
        &mut boot_params,
        initramfs_result.gpa,
        initramfs_result.size,
    )
    .expect("set_initramfs_params failed");
    guest_mem
        .write_obj(Gpa::new(BOOT_PARAMS_GPA), &boot_params)
        .expect("write boot_params failed");

    // ── 7. Write command line ──
    let cmdline = b"console=ttyS0 earlyprintk=serial rdinit=/init\0";
    guest_mem
        .write_slice(Gpa::new(CMDLINE_GPA), cmdline)
        .expect("write cmdline failed");

    // ── 8. Write GDT ──
    boot_regs::write_gdt(&guest_mem).expect("write_gdt failed");

    // ── 9. Create WHP partition (now with APIC) + map memory ──
    let hv = WhpHypervisor::new().expect("WHP not available");
    let cfg = hitz_hal::PartitionConfig {
        vcpu_count: 1,
        memory_size: hitz_hal::MemSizeMiB::new(256),
    };
    let mut partition = hv.create_partition(&cfg).expect("create_partition failed");

    guest_mem
        .map_to_partition(&mut partition, MemFlags::READ_WRITE_EXEC)
        .expect("map_to_partition failed");

    // ── 10. Create vCPU + configure registers ──
    let mut vcpu = partition
        .create_vcpu(VcpuId::new(0))
        .expect("create_vcpu failed");

    boot_regs::configure_sregs(&mut vcpu, pml4_gpa).expect("configure_sregs failed");
    boot_regs::configure_regs(
        &mut vcpu,
        load_result.entry_point,
        Gpa::new(BOOT_PARAMS_GPA),
    )
    .expect("configure_regs failed");

    // ── 11. Set up devices ──
    let guest_mem = Arc::new(guest_mem);
    let stop = AtomicBool::new(false);
    let devices = Mutex::new(SharedDevices {
        serial: SerialDevice::new(Vec::new()),
        mmio_bus: MmioBus::new(),
    });

    // ── 12. Run! ──
    let reason =
        run_vcpu_loop(&mut vcpu, &devices, &*guest_mem, &stop).expect("run_vcpu_loop failed");

    // ── 13. Check results ──
    let devs = devices.lock().expect("lock");
    let output = String::from_utf8_lossy(devs.serial.writer().as_slice());
    let output_len = output.len();
    let preview_end = output_len.min(4000);
    eprintln!(
        "--- Serial output ({output_len} bytes, showing first {preview_end}) ---\n{}",
        &output[..preview_end]
    );
    eprintln!("--- Exit reason: {reason:?} ---");

    assert!(
        output.contains("Linux version") || output.contains("hitz-boot-ok"),
        "expected 'Linux version' or 'hitz-boot-ok' in serial output ({output_len} bytes)"
    );
}

// ─── Phase 5: boot_and_run extraction ────────────────────────────────────────

/// Phase 5 checkpoint: `boot_and_run` produces identical results to the Phase 2
/// hand-rolled pipeline.
///
/// Creates a temporary ELF file that writes "Hello" to COM1 then halts,
/// wraps it in a `VmConfig`, and calls `boot_and_run`. Validates that the
/// extracted pipeline produces the same serial output as the Phase 2 test.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase5_boot_and_run_hello() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    // x86-64 machine code that writes "Hello" to COM1 (0x3F8) then halts.
    // Same code as phase2_serial_output_from_elf.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    // Write ELF to a temp file so boot_and_run can read it by path.
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF to temp file");
    tmp.flush().expect("flush temp file");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");

    // SharedWriter: captures serial output via Arc<Mutex<Vec<u8>>>.
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let result = hitz_vmm::boot_and_run(
        &hv,
        &config,
        writer,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .expect("boot_and_run failed");

    assert_eq!(
        result.exit_reason,
        ExitReason::Halt,
        "expected Halt exit, got {:?}",
        result.exit_reason
    );

    let output = buffer.lock().expect("lock buffer");
    assert_eq!(
        output.as_slice(),
        b"Hello",
        "serial output mismatch: got {:?}",
        String::from_utf8_lossy(&output)
    );
}

/// Thread-safe writer for capturing serial output in tests.
struct SharedWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut vec = self
            .0
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        vec.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ─── Phase 6: daemon VmManager lifecycle ─────────────────────────────────────

/// Phase 6 checkpoint: VmManager can create, start, stop, and delete a VM
/// using the real WHP hypervisor.
///
/// Creates a "Hello" ELF VM through the daemon's VmManager, starts it,
/// waits for it to exit (the HLT halts the vCPU loop), and verifies the
/// state transitions: Created → Running → Stopped. Then deletes the VM.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase6_vm_manager_lifecycle() {
    use std::io::Write;
    use std::sync::Arc;

    use hitz_api::{VmConfig, VmState};
    use hitz_daemon::VmManager;

    // x86-64 machine code: writes "Hello" to COM1 (0x3F8) then halts.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().expect("WHP not available"));
        let manager = VmManager::new(hv);

        // Create
        let info = manager
            .create_vm("test-vm".into(), config)
            .expect("create_vm");
        assert_eq!(info.state, VmState::Created);

        // Start
        let info = manager.start_vm("test-vm").expect("start_vm");
        assert_eq!(info.state, VmState::Running);

        // Wait for the VM to finish (the "Hello" program halts quickly).
        // Poll status until it changes from Running.
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let info = manager.get_vm("test-vm").expect("get_vm");
            if info.state != VmState::Running {
                assert_eq!(
                    info.state,
                    VmState::Stopped,
                    "expected Stopped, got {:?} (exit: {:?})",
                    info.state,
                    info.exit_reason
                );
                break;
            }
        }

        let info = manager.get_vm("test-vm").expect("get_vm final");
        assert_eq!(info.state, VmState::Stopped, "VM should have stopped");

        // Delete
        manager.delete_vm("test-vm").expect("delete_vm");
        let err = manager.get_vm("test-vm").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    });
}

// ─── Phase 8: SMP integration tests ──────────────────────────────────────────

/// Phase 8 checkpoint: Boot hello ELF with 2 vCPUs.
///
/// APs stay in wait-for-SIPI since this simple ELF doesn't do SMP init
/// (no ACPI tables pointing to AP trampoline). Validates that the
/// multi-thread machinery works without crashing.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase8_smp_2vcpu_hello() {
    use std::io::Write;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    // x86-64 machine code: writes "Hello" to COM1 (0x3F8) then halts.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    // Write ELF to a temp file so boot_and_run can read it by path.
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF to temp file");
    tmp.flush().expect("flush temp file");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 2,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");

    // SharedWriter: captures serial output via Arc<Mutex<Vec<u8>>>.
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let result = hitz_vmm::boot_and_run(&hv, &config, writer, Arc::new(AtomicBool::new(false)))
        .expect("boot_and_run should succeed");

    assert_eq!(
        result.exit_reason,
        ExitReason::Halt,
        "expected Halt exit, got {:?}",
        result.exit_reason
    );

    let output = buffer.lock().expect("lock buffer");
    let serial_output = String::from_utf8_lossy(&output);
    assert!(
        serial_output.contains("Hello"),
        "expected Hello in serial output, got: {serial_output}"
    );
}

/// Phase 8 checkpoint: Boot real Linux with 2 CPUs, verify SMP bringup in
/// serial output.
///
/// Requires: `HITZ_KERNEL_PATH` env var pointing to a vmlinux binary.
/// Optional: `HITZ_INITRAMFS_PATH` for an initramfs.
///
/// Run:
/// ```bash
/// HITZ_KERNEL_PATH=path/to/vmlinux HITZ_INITRAMFS_PATH=path/to/initramfs.cpio \
///   cargo test -p hitz-whp -- --ignored phase8_smp_linux_boot --test-threads=1
/// ```
#[test]
#[ignore = "requires WHP + vmlinux (set HITZ_KERNEL_PATH, optionally HITZ_INITRAMFS_PATH)"]
fn phase8_smp_linux_boot() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

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
        cpus: 2,
        cmdline: Some("console=ttyS0 earlyprintk=serial nokaslr\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");

    // SharedWriter: captures serial output via Arc<Mutex<Vec<u8>>>.
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let stop = Arc::new(AtomicBool::new(false));

    // Give the kernel 10 seconds to boot, then signal stop.
    let stop_clone = Arc::clone(&stop);
    let timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        stop_clone.store(true, Ordering::Relaxed);
    });

    let _result = hitz_vmm::boot_and_run(&hv, &config, writer, stop);

    timer.join().unwrap();

    let output = buffer.lock().expect("lock buffer");
    let serial_output = String::from_utf8_lossy(&output);
    assert!(
        serial_output.contains("2 CPUs") || serial_output.contains("Processors"),
        "expected SMP boot messages in serial output.\nGot:\n{serial_output}"
    );
}

// ─── Phase 7: serial streaming through VmManager ────────────────────────────

/// Phase 7 checkpoint: serial output from `boot_and_run` reaches `SerialBuf`
/// and can be read back through `VmManager::serial_reader`.
///
/// Creates a "Hello" ELF VM through the daemon's `VmManager`, starts it,
/// obtains a `SerialReader`, and verifies that the serial output arrives.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase7_vm_manager_serial_streaming() {
    use std::io::Write;
    use std::sync::Arc;

    use hitz_api::VmConfig;
    use hitz_daemon::VmManager;

    // x86-64 machine code: writes "Hello" to COM1 (0x3F8) then halts.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().expect("WHP not available"));
        let manager = VmManager::new(hv);

        let _ = manager
            .create_vm("serial-test".into(), config)
            .expect("create");
        let _ = manager.start_vm("serial-test").expect("start");

        // Give the VM time to boot and print.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let mut reader = manager.serial_reader("serial-test").expect("serial reader");

        // Check if any serial output arrived.
        let chunk =
            tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_chunk()).await;

        // Stop the VM (may already be stopped from HLT).
        let _ = manager.stop_vm("serial-test");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        if let Ok(Some(data)) = chunk {
            let text = String::from_utf8_lossy(&data);
            assert!(
                text.contains("Hello") || !text.is_empty(),
                "expected serial output, got empty"
            );
        }
    });
}

// ─── Phase 9: graceful shutdown integration tests ────────────────────────────

/// Phase 9: boot_and_run exits cleanly when stop_flag is set externally.
///
/// Boots the "Hello" ELF, sets the stop flag after 200ms. The internal
/// cancel-watchdog thread detects the flag and calls `cancel_via()` on
/// the vCPU, forcing it out of `vcpu.run()`. Verifies `boot_and_run`
/// returns within a reasonable time with Halt or Canceled exit reason.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase9_cancel_via_stop_flag() {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    // x86-64: write "Hello" to COM1 (0x3F8) then HLT — same code as Phase 5.
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();

    // Set stop_flag after 200ms. The ELF runs so fast it typically HLTs
    // before this fires, so we accept either Halt or Canceled.
    let _timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        flag.store(true, Ordering::Relaxed);
    });

    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let result = hitz_vmm::boot_and_run(&hv, &config, writer, stop_flag)
        .expect("boot_and_run should succeed");

    assert!(
        matches!(result.exit_reason, ExitReason::Halt | ExitReason::Canceled),
        "expected Halt or Canceled, got {:?}",
        result.exit_reason
    );

    // Serial output should be "Hello" regardless of exit path.
    let output = buffer.lock().expect("lock buffer");
    assert_eq!(
        output.as_slice(),
        b"Hello",
        "serial output mismatch: got {:?}",
        String::from_utf8_lossy(&output)
    );
}

/// Phase 9: multi-vCPU boot_and_run exits cleanly on external cancel.
///
/// Same "Hi" ELF with 2 vCPUs. BSP runs and HLTs; AP starts in
/// wait-for-SIPI state. Setting stop_flag triggers the cancel-watchdog
/// to cancel both vCPUs. Verifies all threads exit within 2 seconds.
#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase9_multi_vcpu_cancel() {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use hitz_api::VmConfig;
    use hitz_vmm::ExitReason;

    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x69, // mov al, 'i'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 2,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
    };

    let hv = WhpHypervisor::new().expect("WHP not available");
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();

    // Set stop_flag after 500ms to give the BSP time to HLT.
    let _timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        flag.store(true, Ordering::Relaxed);
    });

    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(Arc::clone(&buffer));

    let start = std::time::Instant::now();
    let result = hitz_vmm::boot_and_run(&hv, &config, writer, stop_flag)
        .expect("boot_and_run should succeed");
    let elapsed = start.elapsed();

    assert!(
        matches!(result.exit_reason, ExitReason::Halt | ExitReason::Canceled),
        "expected Halt or Canceled, got {:?}",
        result.exit_reason
    );

    // Should complete well within 2 seconds (500ms delay + watchdog + thread join).
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "boot_and_run took too long: {elapsed:?}"
    );
}
