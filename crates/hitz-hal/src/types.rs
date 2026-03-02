//! Shared types for the HAL: exit reasons, register state, configuration.

use crate::newtypes::{Gpa, MemSizeMiB, VcpuId};

/// Configuration for creating a new partition.
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Number of virtual CPUs.
    pub vcpu_count: u32,
    /// Total guest RAM in MiB.
    pub memory_size: MemSizeMiB,
}

/// Reason a vCPU exited the run loop.
#[derive(Debug)]
pub enum VcpuExit {
    /// Guest performed an MMIO access.
    Mmio(MmioExit),
    /// Guest performed an I/O port access (IN/OUT instruction).
    IoPort(IoPortExit),
    /// Guest executed HLT.
    Halt,
    /// vCPU is ready to accept an interrupt injection.
    InterruptWindow,
    /// vCPU run was canceled via [`Vcpu::cancel`](crate::Vcpu::cancel).
    Canceled,
    /// Guest initiated shutdown (triple fault, etc.).
    Shutdown,
    /// Exit reason not recognized by this HAL version.
    Unknown(u32),
}

/// Details of an MMIO exit.
#[derive(Debug)]
pub struct MmioExit {
    /// Guest physical address being accessed.
    pub gpa: Gpa,
    /// Data buffer — for writes, contains the value written by the guest.
    /// For reads, the VMM fills this before resuming.
    pub data: [u8; 8],
    /// Number of valid bytes in `data` (1, 2, 4, or 8).
    pub len: u8,
    /// `true` = guest is writing to the address, `false` = reading.
    pub is_write: bool,
    /// Length of the faulting instruction in bytes.
    /// The VMM must advance RIP by this amount before re-entering the guest.
    pub instruction_len: u8,
}

/// Details of an I/O port exit.
#[derive(Debug)]
pub struct IoPortExit {
    /// Port number (0x0000–0xFFFF).
    pub port: u16,
    /// Data buffer — for OUT, contains the value written by the guest.
    /// For IN, the VMM fills this before resuming.
    pub data: [u8; 4],
    /// Number of valid bytes in `data` (1, 2, or 4).
    pub len: u8,
    /// `true` = guest is writing (OUT), `false` = reading (IN).
    pub is_write: bool,
    /// Length of the faulting instruction in bytes.
    /// The VMM must advance RIP by this amount before re-entering the guest.
    pub instruction_len: u8,
}

/// x86-64 general-purpose registers + RIP, RFLAGS.
#[derive(Debug, Clone, Default)]
pub struct StandardRegs {
    /// Instruction pointer.
    pub rip: u64,
    /// Flags register.
    pub rflags: u64,
    /// General-purpose registers.
    pub rax: u64,
    /// General-purpose register.
    pub rbx: u64,
    /// General-purpose register.
    pub rcx: u64,
    /// General-purpose register.
    pub rdx: u64,
    /// General-purpose register.
    pub rsi: u64,
    /// General-purpose register.
    pub rdi: u64,
    /// Stack pointer.
    pub rsp: u64,
    /// Base pointer.
    pub rbp: u64,
    /// General-purpose register.
    pub r8: u64,
    /// General-purpose register.
    pub r9: u64,
    /// General-purpose register.
    pub r10: u64,
    /// General-purpose register.
    pub r11: u64,
    /// General-purpose register.
    pub r12: u64,
    /// General-purpose register.
    pub r13: u64,
    /// General-purpose register.
    pub r14: u64,
    /// General-purpose register.
    pub r15: u64,
}

/// x86-64 segment descriptor for use in segment registers.
#[derive(Debug, Clone, Default)]
pub struct SegmentDescriptor {
    /// Base address.
    pub base: u64,
    /// Segment limit.
    pub limit: u32,
    /// Selector value.
    pub selector: u16,
    /// Type field.
    pub type_: u8,
    /// Present bit.
    pub present: u8,
    /// Descriptor privilege level.
    pub dpl: u8,
    /// Default operation size (0 = 16-bit, 1 = 32-bit).
    pub db: u8,
    /// Granularity (0 = byte, 1 = 4 KiB).
    pub granularity: u8,
    /// Long mode (64-bit code segment).
    pub long_mode: u8,
    /// System descriptor (0 = system, 1 = code/data).
    pub s: u8,
}

/// Descriptor table register (GDTR/IDTR).
#[derive(Debug, Clone, Default)]
pub struct DescriptorTable {
    /// Base address.
    pub base: u64,
    /// Limit (size - 1).
    pub limit: u16,
}

/// x86-64 special (system) registers: control registers, segment registers, etc.
#[derive(Debug, Clone, Default)]
pub struct SpecialRegs {
    /// CR0 — contains PE (protection enable), PG (paging), etc.
    pub cr0: u64,
    /// CR3 — page table root physical address.
    pub cr3: u64,
    /// CR4 — PAE, PSE, OSFXSR, etc.
    pub cr4: u64,
    /// EFER — long mode enable, NXE, SCE.
    pub efer: u64,

    /// Code segment.
    pub cs: SegmentDescriptor,
    /// Data segment.
    pub ds: SegmentDescriptor,
    /// Extra segment.
    pub es: SegmentDescriptor,
    /// FS segment.
    pub fs: SegmentDescriptor,
    /// GS segment.
    pub gs: SegmentDescriptor,
    /// Stack segment.
    pub ss: SegmentDescriptor,
    /// Task register.
    pub tr: SegmentDescriptor,
    /// LDT register.
    pub ldt: SegmentDescriptor,

    /// Global Descriptor Table register.
    pub gdt: DescriptorTable,
    /// Interrupt Descriptor Table register.
    pub idt: DescriptorTable,
}

/// Flags controlling memory mapping permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemFlags {
    /// Guest can read from this region.
    pub read: bool,
    /// Guest can write to this region.
    pub write: bool,
    /// Guest can execute code from this region.
    pub execute: bool,
}

impl MemFlags {
    /// Read-only memory.
    pub const READ_ONLY: Self = Self {
        read: true,
        write: false,
        execute: false,
    };

    /// Read-write memory (most common for RAM).
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };

    /// Read-write-execute memory (for code regions).
    pub const READ_WRITE_EXEC: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
}

/// Information about an interrupt to inject into a vCPU.
#[derive(Debug, Clone, Copy)]
pub struct InterruptRequest {
    /// Target vCPU.
    pub vcpu_id: VcpuId,
    /// Interrupt vector number.
    pub vector: u8,
}
