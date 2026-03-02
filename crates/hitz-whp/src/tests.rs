//! WHP lifecycle integration tests.
//!
//! These tests require Windows Hypervisor Platform to be enabled.
//! Run with: `cargo test -p hitz-whp -- --ignored` to include WHP tests.
//!
//! To enable WHP: Settings > Apps > Optional Features > More Windows Features >
//! check "Windows Hypervisor Platform", reboot.

// Tests use expect() liberally — panicking on failure is the point.
// RAM sizes are u64 but add_region takes usize; safe on 64-bit Windows.
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

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
    use hitz_devices::serial::SerialDevice;
    use hitz_vmm::run_loop::{ExitReason, run_vcpu_loop};

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

    let (_partition, mut vcpu, _guest_mem) = boot_elf_pipeline(code);
    let mut serial = SerialDevice::new(Vec::new());

    let reason = run_vcpu_loop(&mut vcpu, &mut serial).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");
    assert_eq!(
        serial.writer().as_slice(),
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
    use hitz_devices::serial::SerialDevice;
    use hitz_vmm::run_loop::{ExitReason, run_vcpu_loop};

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

    let (_partition, mut vcpu, _guest_mem) = boot_elf_pipeline(code);
    let mut serial = SerialDevice::new(Vec::new());

    let reason = run_vcpu_loop(&mut vcpu, &mut serial).expect("run_vcpu_loop failed");

    assert_eq!(reason, ExitReason::Halt, "expected Halt exit");

    // LSR default = THRE (0x20) | TEMT (0x40) = 0x60
    let output = serial.writer();
    assert_eq!(output.len(), 1, "expected 1 byte of serial output");
    assert_eq!(
        output[0], 0x60,
        "expected LSR = THRE|TEMT (0x60), got {:#x}",
        output[0]
    );
}
