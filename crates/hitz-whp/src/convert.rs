//! Conversions between HAL types and WHP types.
//!
//! All the register name arrays and value ↔ struct mappings live here,
//! keeping the partition and vCPU modules clean.

use hitz_hal::{
    DescriptorTable, Gpa, HalError, IoPortExit, MmioExit, SegmentDescriptor, SpecialRegs,
    StandardRegs, VcpuExit,
};
use windows::Win32::System::Hypervisor::{
    WHV_MEMORY_ACCESS_INFO_0, WHV_REGISTER_NAME, WHV_REGISTER_VALUE, WHV_RUN_VP_EXIT_CONTEXT,
    WHV_UINT128, WHV_UINT128_0, WHV_X64_IO_PORT_ACCESS_INFO_0, WHV_X64_SEGMENT_REGISTER,
    WHV_X64_TABLE_REGISTER, WHvRunVpExitReasonCanceled, WHvRunVpExitReasonMemoryAccess,
    WHvRunVpExitReasonUnrecoverableException, WHvRunVpExitReasonX64Halt,
    WHvRunVpExitReasonX64InterruptWindow, WHvRunVpExitReasonX64IoPortAccess, WHvX64RegisterCr0,
    WHvX64RegisterCr3, WHvX64RegisterCr4, WHvX64RegisterCs, WHvX64RegisterDs, WHvX64RegisterEfer,
    WHvX64RegisterEs, WHvX64RegisterFs, WHvX64RegisterGdtr, WHvX64RegisterGs, WHvX64RegisterIdtr,
    WHvX64RegisterLdtr, WHvX64RegisterR8, WHvX64RegisterR9, WHvX64RegisterR10, WHvX64RegisterR11,
    WHvX64RegisterR12, WHvX64RegisterR13, WHvX64RegisterR14, WHvX64RegisterR15, WHvX64RegisterRax,
    WHvX64RegisterRbp, WHvX64RegisterRbx, WHvX64RegisterRcx, WHvX64RegisterRdi, WHvX64RegisterRdx,
    WHvX64RegisterRflags, WHvX64RegisterRip, WHvX64RegisterRsi, WHvX64RegisterRsp,
    WHvX64RegisterSs, WHvX64RegisterTr,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Create a `WHV_REGISTER_VALUE` with a 64-bit value, zero-extending to 128 bits.
///
/// Using `{ Reg64: val }` only writes 8 of 16 bytes in the union — the upper
/// 8 bytes contain stack garbage. WHP reads all 16 bytes per register, so
/// uninitialized data there causes `ACCESS_VIOLATION`. Writing via `Reg128`
/// ensures the full union is initialized.
pub const fn reg64_val(val: u64) -> WHV_REGISTER_VALUE {
    WHV_REGISTER_VALUE {
        Reg128: WHV_UINT128 {
            Anonymous: WHV_UINT128_0 {
                Low64: val,
                High64: 0,
            },
        },
    }
}

// ─── Standard (general-purpose) registers ────────────────────────────────────

/// Register names for standard (GP) register get/set, in the same order
/// as the fields in [`StandardRegs`].
pub const STANDARD_REG_NAMES: [WHV_REGISTER_NAME; 18] = [
    WHvX64RegisterRip,
    WHvX64RegisterRflags,
    WHvX64RegisterRax,
    WHvX64RegisterRbx,
    WHvX64RegisterRcx,
    WHvX64RegisterRdx,
    WHvX64RegisterRsi,
    WHvX64RegisterRdi,
    WHvX64RegisterRsp,
    WHvX64RegisterRbp,
    WHvX64RegisterR8,
    WHvX64RegisterR9,
    WHvX64RegisterR10,
    WHvX64RegisterR11,
    WHvX64RegisterR12,
    WHvX64RegisterR13,
    WHvX64RegisterR14,
    WHvX64RegisterR15,
];

/// Convert WHP register values to HAL `StandardRegs`.
///
/// # Safety
///
/// Caller must ensure all 18 values were populated by `WHvGetVirtualProcessorRegisters`
/// with matching `STANDARD_REG_NAMES`.
pub const unsafe fn values_to_standard_regs(v: &[WHV_REGISTER_VALUE; 18]) -> StandardRegs {
    StandardRegs {
        rip: unsafe { v[0].Reg64 },
        rflags: unsafe { v[1].Reg64 },
        rax: unsafe { v[2].Reg64 },
        rbx: unsafe { v[3].Reg64 },
        rcx: unsafe { v[4].Reg64 },
        rdx: unsafe { v[5].Reg64 },
        rsi: unsafe { v[6].Reg64 },
        rdi: unsafe { v[7].Reg64 },
        rsp: unsafe { v[8].Reg64 },
        rbp: unsafe { v[9].Reg64 },
        r8: unsafe { v[10].Reg64 },
        r9: unsafe { v[11].Reg64 },
        r10: unsafe { v[12].Reg64 },
        r11: unsafe { v[13].Reg64 },
        r12: unsafe { v[14].Reg64 },
        r13: unsafe { v[15].Reg64 },
        r14: unsafe { v[16].Reg64 },
        r15: unsafe { v[17].Reg64 },
    }
}

/// Convert HAL `StandardRegs` to WHP register values.
pub const fn standard_regs_to_values(regs: &StandardRegs) -> [WHV_REGISTER_VALUE; 18] {
    [
        reg64_val(regs.rip),
        reg64_val(regs.rflags),
        reg64_val(regs.rax),
        reg64_val(regs.rbx),
        reg64_val(regs.rcx),
        reg64_val(regs.rdx),
        reg64_val(regs.rsi),
        reg64_val(regs.rdi),
        reg64_val(regs.rsp),
        reg64_val(regs.rbp),
        reg64_val(regs.r8),
        reg64_val(regs.r9),
        reg64_val(regs.r10),
        reg64_val(regs.r11),
        reg64_val(regs.r12),
        reg64_val(regs.r13),
        reg64_val(regs.r14),
        reg64_val(regs.r15),
    ]
}

// ─── Special (system) registers ──────────────────────────────────────────────

/// Number of special registers we get/set.
pub const SPECIAL_REG_COUNT: usize = 14;

/// Register names for special register get/set.
/// Order: CR0, CR3, CR4, EFER, CS, DS, ES, FS, GS, SS, TR, LDTR, GDT, IDT.
pub const SPECIAL_REG_NAMES: [WHV_REGISTER_NAME; SPECIAL_REG_COUNT] = [
    WHvX64RegisterCr0,
    WHvX64RegisterCr3,
    WHvX64RegisterCr4,
    WHvX64RegisterEfer,
    WHvX64RegisterCs,
    WHvX64RegisterDs,
    WHvX64RegisterEs,
    WHvX64RegisterFs,
    WHvX64RegisterGs,
    WHvX64RegisterSs,
    WHvX64RegisterTr,
    WHvX64RegisterLdtr,
    WHvX64RegisterGdtr,
    WHvX64RegisterIdtr,
];

/// Convert WHP register values to HAL `SpecialRegs`.
pub const fn values_to_special_regs(v: &[WHV_REGISTER_VALUE; SPECIAL_REG_COUNT]) -> SpecialRegs {
    SpecialRegs {
        cr0: unsafe { v[0].Reg64 },
        cr3: unsafe { v[1].Reg64 },
        cr4: unsafe { v[2].Reg64 },
        efer: unsafe { v[3].Reg64 },
        cs: whp_seg_to_hal(unsafe { &v[4].Segment }),
        ds: whp_seg_to_hal(unsafe { &v[5].Segment }),
        es: whp_seg_to_hal(unsafe { &v[6].Segment }),
        fs: whp_seg_to_hal(unsafe { &v[7].Segment }),
        gs: whp_seg_to_hal(unsafe { &v[8].Segment }),
        ss: whp_seg_to_hal(unsafe { &v[9].Segment }),
        tr: whp_seg_to_hal(unsafe { &v[10].Segment }),
        ldt: whp_seg_to_hal(unsafe { &v[11].Segment }),
        gdt: whp_table_to_hal(unsafe { &v[12].Table }),
        idt: whp_table_to_hal(unsafe { &v[13].Table }),
    }
}

/// Convert HAL `SpecialRegs` to WHP register values.
///
/// The array is zero-initialized first, then each element is overwritten.
/// This ensures no uninitialized padding bytes in the `WHV_REGISTER_VALUE`
/// union, which can cause `ACCESS_VIOLATION` when passed as a batch to
/// `WHvSetVirtualProcessorRegisters`.
pub fn special_regs_to_values(sregs: &SpecialRegs) -> [WHV_REGISTER_VALUE; SPECIAL_REG_COUNT] {
    // Zero the entire array first so no union variant leaves garbage bytes.
    let mut vals: [WHV_REGISTER_VALUE; SPECIAL_REG_COUNT] = unsafe { core::mem::zeroed() };

    vals[0] = reg64_val(sregs.cr0);
    vals[1] = reg64_val(sregs.cr3);
    vals[2] = reg64_val(sregs.cr4);
    vals[3] = reg64_val(sregs.efer);
    vals[4].Segment = hal_seg_to_whp(&sregs.cs);
    vals[5].Segment = hal_seg_to_whp(&sregs.ds);
    vals[6].Segment = hal_seg_to_whp(&sregs.es);
    vals[7].Segment = hal_seg_to_whp(&sregs.fs);
    vals[8].Segment = hal_seg_to_whp(&sregs.gs);
    vals[9].Segment = hal_seg_to_whp(&sregs.ss);
    vals[10].Segment = hal_seg_to_whp(&sregs.tr);
    vals[11].Segment = hal_seg_to_whp(&sregs.ldt);
    vals[12].Table = hal_table_to_whp(&sregs.gdt);
    vals[13].Table = hal_table_to_whp(&sregs.idt);

    vals
}

// ─── Segment / table conversions ─────────────────────────────────────────────

/// Convert a WHP segment register to HAL `SegmentDescriptor`.
const fn whp_seg_to_hal(seg: &WHV_X64_SEGMENT_REGISTER) -> SegmentDescriptor {
    // WHP Attributes bitfield layout (from WinHvPlatformDefs.h):
    //   bits  0-3: SegmentType
    //   bit     4: NonSystemSegment (S)
    //   bits  5-6: DescriptorPrivilegeLevel (DPL)
    //   bit     7: Present
    //   bits 8-11: Reserved (4 bits — NOT the same as raw GDT flags!)
    //   bit    12: Available (AVL)
    //   bit    13: Long (L)
    //   bit    14: Default (D/B)
    //   bit    15: Granularity (G)
    let attrs = unsafe { seg.Anonymous.Anonymous._bitfield };
    SegmentDescriptor {
        base: seg.Base,
        limit: seg.Limit,
        selector: seg.Selector,
        type_: (attrs & 0xF) as u8,
        s: ((attrs >> 4) & 1) as u8,
        dpl: ((attrs >> 5) & 3) as u8,
        present: ((attrs >> 7) & 1) as u8,
        long_mode: ((attrs >> 13) & 1) as u8,
        db: ((attrs >> 14) & 1) as u8,
        granularity: ((attrs >> 15) & 1) as u8,
    }
}

/// Convert a HAL `SegmentDescriptor` to WHP segment register.
fn hal_seg_to_whp(seg: &SegmentDescriptor) -> WHV_X64_SEGMENT_REGISTER {
    // See whp_seg_to_hal for the WHP Attributes bitfield layout.
    // AVL/L/D/G are at bits 12-15, NOT bits 8-11 like in a raw GDT entry.
    let attrs: u16 = u16::from(seg.type_ & 0xF)
        | (u16::from(seg.s & 1) << 4)
        | (u16::from(seg.dpl & 3) << 5)
        | (u16::from(seg.present & 1) << 7)
        | (u16::from(seg.long_mode & 1) << 13)
        | (u16::from(seg.db & 1) << 14)
        | (u16::from(seg.granularity & 1) << 15);

    WHV_X64_SEGMENT_REGISTER {
        Base: seg.base,
        Limit: seg.limit,
        Selector: seg.selector,
        Anonymous: windows::Win32::System::Hypervisor::WHV_X64_SEGMENT_REGISTER_0 {
            Attributes: attrs,
        },
    }
}

/// Convert a WHP table register to HAL `DescriptorTable`.
const fn whp_table_to_hal(table: &WHV_X64_TABLE_REGISTER) -> DescriptorTable {
    DescriptorTable {
        base: table.Base,
        limit: table.Limit,
    }
}

/// Convert a HAL `DescriptorTable` to WHP table register.
const fn hal_table_to_whp(table: &DescriptorTable) -> WHV_X64_TABLE_REGISTER {
    WHV_X64_TABLE_REGISTER {
        Pad: [0; 3],
        Limit: table.limit,
        Base: table.base,
    }
}

// ─── Exit context conversion ─────────────────────────────────────────────────

/// Convert a WHP exit context to a HAL `VcpuExit`.
#[allow(clippy::cast_sign_loss)] // WHV_RUN_VP_EXIT_REASON is i32, our enum uses u32
#[allow(clippy::unnecessary_wraps)] // Will return Err in later phases (emulator failures)
pub fn exit_context_to_hal(ctx: &WHV_RUN_VP_EXIT_CONTEXT) -> Result<VcpuExit, HalError> {
    let reason = ctx.ExitReason;

    // InstructionLength is in the low 4 bits of VpContext._bitfield.
    // WHP does NOT auto-advance RIP on I/O or MMIO exits — the VMM must
    // add this to RIP before re-entering the guest.
    let instruction_len = ctx.VpContext._bitfield & 0x0F;

    if reason == WHvRunVpExitReasonMemoryAccess {
        let mem = unsafe { &ctx.Anonymous.MemoryAccess };
        let access_info = unsafe { mem.AccessInfo.Anonymous };
        let is_write = is_memory_write(access_info);
        let data = [0u8; 8];

        // Copy the raw instruction bytes from the WHP exit context.
        // The MMIO decoder uses these to determine register and access size.
        let instruction_byte_count = mem.InstructionByteCount;
        let mut instruction_bytes = [0u8; 16];
        let count = usize::from(instruction_byte_count).min(16);
        instruction_bytes[..count].copy_from_slice(&mem.InstructionBytes[..count]);

        Ok(VcpuExit::Mmio(MmioExit {
            gpa: Gpa::new(mem.Gpa),
            data,
            len: 0, // Determined by MMIO decoder from instruction bytes
            is_write,
            instruction_len,
            instruction_bytes,
            instruction_byte_count,
        }))
    } else if reason == WHvRunVpExitReasonX64IoPortAccess {
        let io = unsafe { &ctx.Anonymous.IoPortAccess };
        let access_info = unsafe { io.AccessInfo.Anonymous };
        let is_write = is_io_write(access_info);
        let access_size = io_access_size(access_info);
        if access_size > 4 {
            return Err(HalError::VcpuRun(format!(
                "invalid I/O port access size: {access_size}"
            )));
        }
        let mut data = [0u8; 4];
        if is_write {
            // For OUT instructions, the data is in RAX.
            let rax_bytes = io.Rax.to_le_bytes();
            let copy_len = usize::from(access_size).min(4);
            data[..copy_len].copy_from_slice(&rax_bytes[..copy_len]);
        }
        Ok(VcpuExit::IoPort(IoPortExit {
            port: io.PortNumber,
            data,
            len: access_size,
            is_write,
            instruction_len,
        }))
    } else if reason == WHvRunVpExitReasonX64Halt {
        Ok(VcpuExit::Halt)
    } else if reason == WHvRunVpExitReasonX64InterruptWindow {
        Ok(VcpuExit::InterruptWindow)
    } else if reason == WHvRunVpExitReasonCanceled {
        Ok(VcpuExit::Canceled)
    } else if reason == WHvRunVpExitReasonUnrecoverableException {
        Ok(VcpuExit::Shutdown)
    } else {
        Ok(VcpuExit::Unknown(reason.0 as u32))
    }
}

/// Check if a memory access is a write.
///
/// `WHV_MEMORY_ACCESS_INFO.AccessType` is a 2-bit field in bits \[0:1\]:
///   0 = Read, 1 = Write, 2 = Execute.
const fn is_memory_write(info: WHV_MEMORY_ACCESS_INFO_0) -> bool {
    (info._bitfield & 0b11) == 1
}

/// Check if an I/O port access is a write (bit 0 of the bitfield).
const fn is_io_write(info: WHV_X64_IO_PORT_ACCESS_INFO_0) -> bool {
    (info._bitfield & 1) != 0
}

/// Get the access size from an I/O port access info (bits 1-3).
#[allow(clippy::cast_possible_truncation)] // value is masked to 3 bits, always fits u8
const fn io_access_size(info: WHV_X64_IO_PORT_ACCESS_INFO_0) -> u8 {
    ((info._bitfield >> 1) & 0b111) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_error_when_io_access_size_invalid() {
        let mut ctx: WHV_RUN_VP_EXIT_CONTEXT = unsafe { core::mem::zeroed() };
        ctx.ExitReason = WHvRunVpExitReasonX64IoPortAccess;
        ctx.Anonymous.IoPortAccess.AccessInfo.Anonymous._bitfield = (5 << 1) | 1; // write, size 5
        ctx.Anonymous.IoPortAccess.Rax = 0x1122_3344_5566_7788;

        let res = exit_context_to_hal(&ctx);
        assert!(res.is_err());
        if let Err(e) = res {
            assert!(e.to_string().contains("invalid I/O port access size: 5"));
        }
    }
}
