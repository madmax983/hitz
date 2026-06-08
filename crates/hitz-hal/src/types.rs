//! Shared types for the HAL: exit reasons, register state, configuration.

use crate::newtypes::{Gpa, MemSizeMiB, VcpuId};

/// Configuration for creating a new partition.
///
/// # Abstract
///
/// Defines the fundamental parameters required to bootstrap a new VM.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{PartitionConfig, MemSizeMiB};
///
/// let config = PartitionConfig {
///     vcpu_count: 2,
///     memory_size: MemSizeMiB::new(1024),
/// };
/// ```
///
/// # The Fine Print
/// Some hypervisors may round `memory_size` up to the nearest page boundary.
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Number of virtual CPUs.
    pub vcpu_count: u32,
    /// Total guest RAM in MiB.
    pub memory_size: MemSizeMiB,
}

/// Reason a vCPU exited the run loop.
///
/// # Abstract
///
/// Represents the various reasons a virtual CPU might stop execution
/// and return control to the Virtual Machine Monitor (VMM).
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::VcpuExit;
///
/// fn handle_exit(exit: VcpuExit) {
///     match exit {
///         VcpuExit::Halt => println!("Guest halted"),
///         VcpuExit::Shutdown => println!("Guest shutting down"),
///         _ => println!("Other exit reason"),
///     }
/// }
/// ```
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

impl VcpuExit {
    /// Returns a static string label representing the exit reason.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::IoPort(_) => "IoPort",
            Self::Halt => "Halt",
            Self::Shutdown => "Shutdown",
            Self::Mmio(_) => "Mmio",
            Self::InterruptWindow => "InterruptWindow",
            Self::Canceled => "Canceled",
            Self::Unknown(_) => "Unexpected",
        }
    }
}

/// Details of an MMIO exit.
///
/// # Abstract
///
/// Provides the necessary context when the guest accesses memory-mapped I/O,
/// allowing the VMM to emulate the device behavior.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{MmioExit, Gpa};
///
/// let exit = MmioExit {
///     gpa: Gpa::new(0x1000),
///     data: [0; 8],
///     len: 4,
///     is_write: false,
///     instruction_len: 3,
///     instruction_bytes: [0; 16],
///     instruction_byte_count: 0,
/// };
///
/// if exit.is_write {
///     println!("Guest writing to MMIO");
/// }
/// ```
///
/// # The Fine Print
/// `data` holds up to 8 bytes. For smaller accesses, only the lower bytes are valid.
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
    /// Raw instruction bytes from WHP's exit context.
    /// Used by the MMIO decoder to determine which register and access size.
    pub instruction_bytes: [u8; 16],
    /// Number of valid bytes in `instruction_bytes`.
    pub instruction_byte_count: u8,
}

/// Details of an I/O port exit.
///
/// # Abstract
///
/// Provides the context when the guest executes an `IN` or `OUT` instruction.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::IoPortExit;
///
/// let exit = IoPortExit {
///     port: 0x3F8, // COM1
///     data: [0x41, 0, 0, 0], // 'A'
///     len: 1,
///     is_write: true,
///     instruction_len: 2,
/// };
///
/// if exit.is_write && exit.port == 0x3F8 {
///     println!("Guest wrote to COM1");
/// }
/// ```
///
/// # The Fine Print
/// Only the lower bytes of `data` are valid (dependent on `len`).
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
///
/// # Abstract
///
/// Represents the standard state of a vCPU that needs to be saved/restored
/// or modified by the VMM.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::StandardRegs;
///
/// let mut regs = StandardRegs::default();
/// regs.rip = 0x100000;
/// regs.rflags = 0x2;
/// ```
///
/// # The Fine Print
/// Ensure `rflags` has the reserved bit 1 set (0x2) as mandated by x86 architecture.
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
///
/// # Abstract
///
/// Defines a segment in the x86 architecture. Used for CS, DS, ES, FS, GS,
/// and SS registers.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::SegmentDescriptor;
///
/// let mut cs = SegmentDescriptor::default();
/// cs.base = 0;
/// cs.limit = 0xFFFFFFFF;
/// cs.type_ = 11; // Execute/Read, accessed
/// ```
///
/// # The Fine Print
/// `type_` and other flags correspond to the standard x86 segment descriptor format.
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
///
/// # Abstract
///
/// Represents the Global or Interrupt Descriptor Table register.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::DescriptorTable;
///
/// let gdtr = DescriptorTable {
///     base: 0x2000,
///     limit: 0x1FF,
/// };
/// ```
///
/// # The Fine Print
/// `limit` is the size of the table minus 1, in bytes.
#[derive(Debug, Clone, Default)]
pub struct DescriptorTable {
    /// Base address.
    pub base: u64,
    /// Limit (size - 1).
    pub limit: u16,
}

/// x86-64 special (system) registers: control registers, segment registers, etc.
///
/// # Abstract
///
/// Holds the state of system-level configuration registers for a vCPU.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::SpecialRegs;
///
/// let mut sregs = SpecialRegs::default();
/// sregs.cr0 = 0x80000011; // Enable Paging + Protection
/// ```
///
/// # The Fine Print
/// These registers represent the architectural configuration. Incorrect values
/// will result in an immediate triple fault upon guest entry.
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
///
/// # Abstract
///
/// Defines access rights (Read, Write, Execute) for a mapped memory region.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::MemFlags;
///
/// let flags = MemFlags::READ_WRITE;
/// assert!(flags.read);
/// assert!(flags.write);
/// assert!(!flags.execute);
/// ```
///
/// # The Fine Print
/// Not all hypervisors support execute-only or write-only memory natively.
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
///
/// # Abstract
///
/// Encapsulates the target vCPU and the vector of the interrupt.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_hal::{InterruptRequest, VcpuId};
///
/// let req = InterruptRequest {
///     vcpu_id: VcpuId::new(0),
///     vector: 32, // e.g., timer interrupt
/// };
/// ```
///
/// # The Fine Print
/// Hardware interrupts are generally edge-triggered.
#[derive(Debug, Clone, Copy)]
pub struct InterruptRequest {
    /// Target vCPU.
    pub vcpu_id: VcpuId,
    /// Interrupt vector number.
    pub vector: u8,
}
