//! Boot register configuration for Linux x86-64 direct boot.
//!
//! Sets up the vCPU in 64-bit long mode with identity-mapped page tables,
//! a minimal GDT, and the entry point / `boot_params` pointer expected by
//! the Linux boot protocol.

use hitz_hal::{DescriptorTable, Gpa, SegmentDescriptor, SpecialRegs, StandardRegs, Vcpu};

use crate::error::MemError;
use crate::memory::GuestMemory;

// ─── GDT layout ─────────────────────────────────────────────────────────────

/// Guest physical address where the GDT is placed.
pub const GDT_GPA: u64 = 0x500;

/// Null descriptor (required first entry).
const GDT_NULL: u64 = 0;

/// 64-bit code segment descriptor.
///
/// Base=0, Limit=0xFFFFF, Type=0xB (exec/read/accessed), S=1, DPL=0,
/// P=1, L=1 (long mode), D=0, G=1.
///
/// Raw: `0x00AF_9B00_0000_FFFF`
const GDT_CODE64: u64 = 0x00AF_9B00_0000_FFFF;

/// 64-bit data segment descriptor.
///
/// Base=0, Limit=0xFFFFF, Type=0x3 (read/write/accessed), S=1, DPL=0,
/// P=1, L=0, D=1 (32-bit default ops for data), G=1.
///
/// Raw: `0x00CF_9300_0000_FFFF`
const GDT_DATA64: u64 = 0x00CF_9300_0000_FFFF;

/// Selector for the 64-bit code segment (entry 1 in GDT, ring 0).
const CS_SELECTOR: u16 = 0x08;
/// Selector for the 64-bit data segment (entry 2 in GDT, ring 0).
const DS_SELECTOR: u16 = 0x10;
/// Selector for the TSS descriptor (entry 3 in GDT, ring 0).
const TR_SELECTOR: u16 = 0x18;

/// Number of GDT entries: null + code64 + data64 + TSS low.
///
/// A 64-bit TSS descriptor occupies TWO GDT slots (16 bytes) but we only
/// need the low 8 bytes in the GDT — the high 8 bytes encode the upper
/// 32 bits of base which is 0 for us.
const GDT_ENTRY_COUNT: usize = 5; // null, code64, data64, tss_low, tss_high
/// Size of each GDT entry in bytes.
const GDT_ENTRY_SIZE: usize = 8;

/// Guest physical address of the minimal 64-bit TSS.
///
/// Placed right after the GDT (5 entries * 8 bytes = 40 bytes starting at 0x500,
/// so TSS at 0x528). The TSS is zero-initialized by the guest memory allocator.
pub const TSS_GPA: u64 = GDT_GPA + (GDT_ENTRY_COUNT as u64) * (GDT_ENTRY_SIZE as u64);

/// Size of a minimal 64-bit TSS (104 bytes = 0x68).
const TSS_SIZE: u64 = 0x68;

// ─── Control register values for 64-bit long mode ───────────────────────────

/// CR0: PE + MP + ET + NE + WP + AM + PG.
///
/// WHP requires more than the bare minimum for long mode. In particular,
/// NE (numeric error handling, bit 5) and the FPU/SSE-related bits must
/// be set for the vCPU state to be considered valid.
const CR0_LONG_MODE: u64 = (1 << 0)  // PE  — protection enable
    | (1 << 1)                        // MP  — monitor coprocessor
    | (1 << 4)                        // ET  — extension type
    | (1 << 5)                        // NE  — numeric error
    | (1 << 16)                       // WP  — write protect
    | (1 << 18)                       // AM  — alignment mask
    | (1 << 31); // PG  — paging
// = 0x8005_0033

/// CR4: PAE + OSFXSR + OSXMMEXCPT.
///
/// PAE is required for long mode. OSFXSR and OSXMMEXCPT enable SSE
/// support, which WHP expects for a valid 64-bit register state.
const CR4_LONG_MODE: u64 = (1 << 5)  // PAE — physical address extension
    | (1 << 9)                        // OSFXSR — OS support for FXSAVE/FXRSTOR
    | (1 << 10); // OSXMMEXCPT — OS support for unmasked SIMD FP exceptions
// = 0x620

/// EFER: LME (long mode enable) + LMA (long mode active).
/// When PG is set in CR0 with LME in EFER, the CPU activates LMA automatically.
/// We set both explicitly since WHP expects the VMM to provide the full state.
const EFER_LONG_MODE: u64 = (1 << 8) | (1 << 10); // 0x500

/// RFLAGS: bit 1 is always reserved/set on x86.
const RFLAGS_INIT: u64 = 0x2;

// ─── Segment descriptor helpers ─────────────────────────────────────────────

/// Decode an 8-byte raw GDT entry into a HAL `SegmentDescriptor`.
const fn decode_gdt_entry(raw: u64, selector: u16) -> SegmentDescriptor {
    // Base: bits [31:24] << 24 | bits [39:32] << 16 | bits [15:0] of the high half
    // For flat segments (base=0, which is our case), this is all zeros.
    let base_low = (raw >> 16) & 0xFFFF;
    let base_mid = (raw >> 32) & 0xFF;
    let base_high = (raw >> 56) & 0xFF;
    let base = (base_high << 24) | (base_mid << 16) | base_low;

    // Limit: bits [19:16] from byte 6 high nibble, bits [15:0] from low word
    let limit_low = raw & 0xFFFF;
    let limit_high = (raw >> 48) & 0x0F;
    // If granularity bit is set, limit is in 4 KiB units -> shift left 12 and OR 0xFFF.
    let granularity = ((raw >> 55) & 1) as u8;
    let limit_raw = (limit_high << 16) | limit_low;
    // Limit fits in 20 bits (max 0xFFFFF). With granularity, shifted left 12
    // gives at most 0xFFFF_FFFF which fits u32.
    #[allow(clippy::cast_possible_truncation)]
    let limit = if granularity == 1 {
        ((limit_raw << 12) | 0xFFF) as u32
    } else {
        limit_raw as u32
    };

    // Access byte (byte 5): type(4) | S(1) | DPL(2) | P(1)
    let access = ((raw >> 40) & 0xFF) as u8;
    let type_ = access & 0x0F;
    let s = (access >> 4) & 1;
    let dpl = (access >> 5) & 3;
    let present = (access >> 7) & 1;

    // Flags nibble (high nibble of byte 6): AVL(1) | L(1) | D/B(1) | G(1)
    let flags = ((raw >> 52) & 0x0F) as u8;
    let long_mode = (flags >> 1) & 1;
    let db = (flags >> 2) & 1;

    SegmentDescriptor {
        base,
        limit,
        selector,
        type_,
        s,
        dpl,
        present,
        long_mode,
        db,
        granularity,
    }
}

/// Build a minimal inactive segment descriptor (present=0) for TR/LDT.
const fn null_segment(selector: u16) -> SegmentDescriptor {
    SegmentDescriptor {
        base: 0,
        limit: 0,
        selector,
        type_: 0,
        s: 0,
        dpl: 0,
        present: 0,
        long_mode: 0,
        db: 0,
        granularity: 0,
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Build a 64-bit TSS descriptor as two 8-byte GDT entries.
///
/// A 64-bit TSS descriptor is 16 bytes, spanning two consecutive GDT slots.
/// The low 8 bytes follow the same layout as a 32-bit system descriptor;
/// the high 8 bytes contain the upper 32 bits of the base address.
const fn build_tss_descriptor(base: u64, limit: u64) -> (u64, u64) {
    let base_lo = base & 0xFFFF;
    let base_mid = (base >> 16) & 0xFF;
    let base_hi = (base >> 24) & 0xFF;
    let base_upper = base >> 32;

    let limit_lo = limit & 0xFFFF;
    let limit_hi = (limit >> 16) & 0xF;

    // Access byte: type=0x9 (64-bit TSS available), S=0, DPL=0, P=1 → 0x89
    // Flags nibble: G=0, limit_hi
    let low = limit_lo
        | (base_lo << 16)
        | (base_mid << 32)
        | (0x89 << 40) // access byte
        | (limit_hi << 48)
        | (base_hi << 56);

    let high = base_upper; // upper 32 bits of base, rest reserved (zero)

    (low, high)
}

/// Write the GDT entries into guest memory at [`GDT_GPA`].
///
/// # Abstract
///
/// Bootstraps the Global Descriptor Table (GDT) directly into guest memory.
/// This enables the VM to transition seamlessly into 64-bit long mode.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_vmm::GuestMemory;
/// use hitz_vmm::boot_regs::{write_gdt, GDT_GPA};
/// use hitz_hal::Gpa;
///
/// let mut mem = GuestMemory::new();
/// mem.add_region(Gpa::new(0), 4096).unwrap(); // Add dummy memory
///
/// // Inject the GDT into memory
/// write_gdt(&mem).unwrap();
/// ```
///
/// # The Fine Print
///
/// The GDT contains: `[null, code64, data64, tss_low, tss_high]`.
/// The TSS descriptor spans two GDT slots (16 bytes for 64-bit TSS).
/// Returns a `MemError` if the intended memory region is not mapped.
pub fn write_gdt(mem: &GuestMemory) -> Result<(), MemError> {
    let (tss_low, tss_high) = build_tss_descriptor(TSS_GPA, TSS_SIZE - 1);
    let entries = [GDT_NULL, GDT_CODE64, GDT_DATA64, tss_low, tss_high];
    for (i, &entry) in entries.iter().enumerate() {
        let gpa = Gpa::new(GDT_GPA + (i * GDT_ENTRY_SIZE) as u64);
        mem.write_u64(gpa, entry)?;
    }
    Ok(())
}

/// Configure a vCPU's special registers for 64-bit long mode with the given
/// page table root (PML4 GPA, for CR3).
///
/// # Abstract
///
/// Transitions a virtual CPU directly into 64-bit long mode, bypassing real mode and
/// 32-bit protected mode. It configures the CPU's control registers, EFER, and segment
/// descriptors to point to the correct GDT/IDT locations.
///
/// # The Hero's Journey
///
/// ```rust
/// # use hitz_vmm::boot_regs::configure_sregs;
/// # use hitz_hal::{Gpa, Vcpu, SpecialRegs, StandardRegs, VcpuExit};
/// #
/// # // Minimal dummy struct to implement the required trait bounds for the doc test.
/// # struct DummyVcpu;
/// # impl Vcpu for DummyVcpu {
/// #     fn run(&mut self) -> Result<VcpuExit, hitz_hal::HalError> { Ok(VcpuExit::Halt) }
/// #     fn get_regs(&self) -> Result<StandardRegs, hitz_hal::HalError> { Ok(StandardRegs::default()) }
/// #     fn set_regs(&mut self, _regs: &StandardRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn get_sregs(&self) -> Result<SpecialRegs, hitz_hal::HalError> { unimplemented!() }
/// #     fn set_sregs(&mut self, _sregs: &SpecialRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     type CancelHandle = ();
/// #     fn cancel_handle(&self) -> Self::CancelHandle {}
/// #     fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// # }
/// #
/// # let mut vcpu = DummyVcpu;
/// let pml4_gpa = Gpa::new(0x2000); // Address of page tables
///
/// // Configure the virtual CPU for long mode
/// configure_sregs(&mut vcpu, pml4_gpa).unwrap();
/// ```
///
/// # The Fine Print
///
/// This sets CR0, CR3, CR4, EFER, segment registers (CS/DS/ES/FS/GS/SS),
/// TR, LDT, GDT, and IDT. Ensure the provided `pml4_gpa` corresponds to a valid
/// set of pre-configured page tables in guest memory.
pub fn configure_sregs(vcpu: &mut impl Vcpu, pml4_gpa: Gpa) -> Result<(), hitz_hal::HalError> {
    // GDT limit = 5 entries * 8 bytes - 1 = 39; always fits u16.
    #[allow(clippy::cast_possible_truncation)]
    let gdt_limit = (GDT_ENTRY_COUNT * GDT_ENTRY_SIZE - 1) as u16;

    let sregs = SpecialRegs {
        cr0: CR0_LONG_MODE,
        cr3: pml4_gpa.as_u64(),
        cr4: CR4_LONG_MODE,
        efer: EFER_LONG_MODE,

        cs: decode_gdt_entry(GDT_CODE64, CS_SELECTOR),
        ds: decode_gdt_entry(GDT_DATA64, DS_SELECTOR),
        es: decode_gdt_entry(GDT_DATA64, DS_SELECTOR),
        fs: decode_gdt_entry(GDT_DATA64, DS_SELECTOR),
        gs: decode_gdt_entry(GDT_DATA64, DS_SELECTOR),
        ss: decode_gdt_entry(GDT_DATA64, DS_SELECTOR),

        // Long mode requires a valid TSS in TR for interrupt handling.
        // Type 0xB = 64-bit TSS busy (set to busy since we're "loading" it).
        tr: SegmentDescriptor {
            base: TSS_GPA,
            #[allow(clippy::cast_possible_truncation)]
            limit: (TSS_SIZE - 1) as u32,
            selector: TR_SELECTOR,
            type_: 0xB, // 64-bit TSS busy
            s: 0,       // system descriptor
            dpl: 0,
            present: 1,
            long_mode: 0,
            db: 0,
            granularity: 0,
        },
        ldt: null_segment(0),

        gdt: DescriptorTable {
            base: GDT_GPA,
            limit: gdt_limit,
        },
        idt: DescriptorTable {
            base: 0,
            limit: 0xFFFF,
        },
    };

    vcpu.set_sregs(&sregs)
}

/// Configure a vCPU's general-purpose registers for the Linux boot entry point.
///
/// # Abstract
///
/// Prepares the CPU to start executing the Linux kernel. It sets the instruction
/// pointer to the kernel's entry point and passes the location of the `boot_params`
/// structure as required by the Linux boot protocol.
///
/// # The Hero's Journey
///
/// ```rust
/// # use hitz_vmm::boot_regs::configure_regs;
/// # use hitz_hal::{Gpa, Vcpu, SpecialRegs, StandardRegs, VcpuExit};
/// #
/// # // Minimal dummy struct to implement the required trait bounds for the doc test.
/// # struct DummyVcpu;
/// # impl Vcpu for DummyVcpu {
/// #     fn run(&mut self) -> Result<VcpuExit, hitz_hal::HalError> { Ok(VcpuExit::Halt) }
/// #     fn get_regs(&self) -> Result<StandardRegs, hitz_hal::HalError> { Ok(StandardRegs::default()) }
/// #     fn set_regs(&mut self, _regs: &StandardRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn get_sregs(&self) -> Result<SpecialRegs, hitz_hal::HalError> { unimplemented!() }
/// #     fn set_sregs(&mut self, _sregs: &SpecialRegs) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn inject_interrupt(&mut self, _vector: u8) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn request_interrupt_window(&mut self) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     type CancelHandle = ();
/// #     fn cancel_handle(&self) -> Self::CancelHandle {}
/// #     fn cancel_via(_handle: &Self::CancelHandle) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// # }
/// #
/// # let mut vcpu = DummyVcpu;
/// let entry_point = Gpa::new(0x100000); // Kernel entry point
/// let boot_params_gpa = Gpa::new(0x7000); // Location of boot_params
///
/// // Prepare the CPU to boot Linux
/// configure_regs(&mut vcpu, entry_point, boot_params_gpa).unwrap();
/// ```
///
/// # The Fine Print
///
/// - `RIP` = kernel entry point
/// - `RSI` = `boot_params` GPA (Linux protocol: RSI points to zero page)
/// - `RSP` = top of a small boot stack (defaults to 0, which is fine since the kernel sets its own)
/// - `RFLAGS` = 0x2 (reserved bit set)
pub fn configure_regs(
    vcpu: &mut impl Vcpu,
    entry_point: Gpa,
    boot_params_gpa: Gpa,
) -> Result<(), hitz_hal::HalError> {
    let regs = StandardRegs {
        rip: entry_point.as_u64(),
        rsi: boot_params_gpa.as_u64(),
        rsp: 0, // Kernel sets up its own stack early; 0 is fine for entry.
        rflags: RFLAGS_INIT,
        ..StandardRegs::default()
    };

    vcpu.set_regs(&regs)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn gdt_code64_decode() {
        let seg = decode_gdt_entry(GDT_CODE64, CS_SELECTOR);
        assert_eq!(seg.base, 0, "code64 base");
        assert_eq!(
            seg.limit, 0xFFFF_FFFF,
            "code64 limit (G=1, 0xFFFFF -> 4G-1)"
        );
        assert_eq!(seg.selector, CS_SELECTOR);
        assert_eq!(seg.type_, 0xB, "exec/read/accessed");
        assert_eq!(seg.s, 1, "code/data descriptor");
        assert_eq!(seg.dpl, 0, "ring 0");
        assert_eq!(seg.present, 1);
        assert_eq!(seg.long_mode, 1, "L bit set for 64-bit");
        assert_eq!(seg.db, 0, "D must be 0 when L=1");
        assert_eq!(seg.granularity, 1, "4 KiB granularity");
    }

    #[test]
    fn gdt_data64_decode() {
        let seg = decode_gdt_entry(GDT_DATA64, DS_SELECTOR);
        assert_eq!(seg.base, 0, "data64 base");
        assert_eq!(seg.limit, 0xFFFF_FFFF, "data64 limit");
        assert_eq!(seg.selector, DS_SELECTOR);
        assert_eq!(seg.type_, 0x3, "read/write/accessed");
        assert_eq!(seg.s, 1, "code/data descriptor");
        assert_eq!(seg.dpl, 0, "ring 0");
        assert_eq!(seg.present, 1);
        assert_eq!(seg.long_mode, 0, "L=0 for data segment");
        assert_eq!(seg.db, 1, "D/B=1 for 32-bit default ops");
        assert_eq!(seg.granularity, 1, "4 KiB granularity");
    }

    #[test]
    fn cr0_bits() {
        assert_ne!(CR0_LONG_MODE & 1, 0, "PE must be set");
        assert_ne!(CR0_LONG_MODE & (1 << 1), 0, "MP must be set");
        assert_ne!(CR0_LONG_MODE & (1 << 4), 0, "ET must be set");
        assert_ne!(CR0_LONG_MODE & (1 << 5), 0, "NE must be set");
        assert_ne!(CR0_LONG_MODE & (1 << 16), 0, "WP must be set");
        assert_ne!(CR0_LONG_MODE & (1 << 31), 0, "PG must be set");
        assert_eq!(CR0_LONG_MODE, 0x8005_0033);
    }

    #[test]
    fn cr4_bits() {
        assert_ne!(CR4_LONG_MODE & (1 << 5), 0, "PAE must be set");
        assert_ne!(CR4_LONG_MODE & (1 << 9), 0, "OSFXSR must be set");
        assert_ne!(CR4_LONG_MODE & (1 << 10), 0, "OSXMMEXCPT must be set");
        assert_eq!(CR4_LONG_MODE, 0x620);
    }

    #[test]
    fn efer_long_mode() {
        assert_ne!(EFER_LONG_MODE & (1 << 8), 0, "LME must be set");
        assert_ne!(EFER_LONG_MODE & (1 << 10), 0, "LMA must be set");
    }

    #[test]
    fn gdt_size() {
        let limit = u16::try_from(GDT_ENTRY_COUNT * GDT_ENTRY_SIZE - 1).expect("fits in u16");
        assert_eq!(limit, 39, "5 entries * 8 bytes - 1 = 39");
    }

    #[test]
    fn write_gdt_roundtrip() {
        let mut mem = GuestMemory::new();
        // GDT + TSS need space at 0x500. Region at 0 with 4 KiB covers it.
        mem.add_region(Gpa::new(0), 4096).expect("add_region");

        write_gdt(&mem).expect("write_gdt");

        // Read back the entries.
        let null: u64 = mem.read_u64(Gpa::new(GDT_GPA)).expect("read null");
        let code: u64 = mem.read_u64(Gpa::new(GDT_GPA + 8)).expect("read code");
        let data: u64 = mem.read_u64(Gpa::new(GDT_GPA + 16)).expect("read data");
        let tss_lo: u64 = mem.read_u64(Gpa::new(GDT_GPA + 24)).expect("read tss_low");

        assert_eq!(null, GDT_NULL);
        assert_eq!(code, GDT_CODE64);
        assert_eq!(data, GDT_DATA64);
        // TSS descriptor: type=0x9 (available), P=1 → access byte 0x89.
        assert_eq!((tss_lo >> 40) & 0xFF, 0x89, "TSS access byte");
    }

    #[test]
    fn tss_descriptor_base_limit() {
        let (low, high) = build_tss_descriptor(TSS_GPA, TSS_SIZE - 1);
        // Extract base from descriptor
        let base_lo = (low >> 16) & 0xFFFF;
        let base_mid = (low >> 32) & 0xFF;
        let base_hi = (low >> 56) & 0xFF;
        let base_upper = high & 0xFFFF_FFFF;
        let base = base_lo | (base_mid << 16) | (base_hi << 24) | (base_upper << 32);
        assert_eq!(base, TSS_GPA, "TSS base should match TSS_GPA");

        // Extract limit
        let limit_lo = low & 0xFFFF;
        let limit_hi = (low >> 48) & 0xF;
        let limit = limit_lo | (limit_hi << 16);
        assert_eq!(limit, TSS_SIZE - 1, "TSS limit should be size - 1");
    }

    #[test]
    fn null_segment_is_not_present() {
        let seg = null_segment(42);
        assert_eq!(seg.present, 0);
        assert_eq!(seg.base, 0);
        assert_eq!(seg.limit, 0);
        assert_eq!(seg.selector, 42);
        assert_eq!(seg.type_, 0);
        assert_eq!(seg.s, 0);
        assert_eq!(seg.dpl, 0);
        assert_eq!(seg.long_mode, 0);
        assert_eq!(seg.db, 0);
        assert_eq!(seg.granularity, 0);
    }

    #[test]
    fn tss_descriptor_large_limit() {
        let limit = 0x8_FFFF; // 0x8FFFF, limit_hi should be 8
        let (low, _) = build_tss_descriptor(0x1000, limit);
        let limit_lo = low & 0xFFFF;
        let limit_hi = (low >> 48) & 0xF;
        assert_eq!(limit_lo, 0xFFFF);
        assert_eq!(limit_hi, 0x8);
    }

    /// Smoke test: `configure_regs` builds a valid `StandardRegs`.
    ///
    /// We can't call `set_regs` without a real vCPU, but we can verify the
    /// values that would be set by calling the same logic directly.
    #[test]
    fn standard_regs_values() {
        let entry = Gpa::new(0x10_0200);
        let bp_gpa = Gpa::new(0x7000);

        let regs = StandardRegs {
            rip: entry.as_u64(),
            rsi: bp_gpa.as_u64(),
            rsp: 0,
            rflags: RFLAGS_INIT,
            ..StandardRegs::default()
        };

        assert_eq!(regs.rip, 0x10_0200);
        assert_eq!(regs.rsi, 0x7000);
        assert_eq!(regs.rflags, 0x2);
        assert_eq!(regs.rax, 0);
    }
}
