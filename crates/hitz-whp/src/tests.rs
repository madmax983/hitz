//! WHP lifecycle integration tests.
//!
//! These tests require Windows Hypervisor Platform to be enabled.
//! Run with: `cargo test -p hitz-whp -- --ignored` to include WHP tests.
//!
//! To enable WHP: Settings > Apps > Optional Features > More Windows Features >
//! check "Windows Hypervisor Platform", reboot.

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
