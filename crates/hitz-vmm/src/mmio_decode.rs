//! MMIO instruction decoder for x86-64.
//!
//! WHP (unlike KVM) does not decode MMIO instructions in-kernel.  Instead
//! it hands the VMM raw instruction bytes from the faulting access.  This
//! module decodes the subset of `MOV` encodings that Linux uses for
//! virtio-MMIO register accesses:
//!
//! | Opcode      | Meaning                      | Size  |
//! |-------------|------------------------------|-------|
//! | `89 /r`     | `MOV r/m32, r32`             | 4     |
//! | `8B /r`     | `MOV r32, r/m32`             | 4     |
//! | `88 /r`     | `MOV r/m8, r8`               | 1     |
//! | `8A /r`     | `MOV r8, r/m8`               | 1     |
//! | `C7 /0`     | `MOV r/m32, imm32`           | 4     |
//! | `66 + …`    | 16-bit operand-size override | 2     |
//! | `REX.W + …` | 64-bit operand-size override | 8     |
//!
//! REX prefixes (`0x40–0x4F`) are handled both for `R8–R15` access (REX.R)
//! and for the 64-bit operand size (REX.W). REX.W takes precedence over the
//! `0x66` prefix when both are present.

use hitz_hal::StandardRegs;

/// Decoded MMIO instruction information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct DecodedMmio {
    /// GP register index 0–15 (x86 encoding order).
    ///
    /// 0=RAX, 1=RCX, 2=RDX, 3=RBX, 4=RSP, 5=RBP, 6=RSI, 7=RDI, 8–15=R8–R15.
    pub register: u8,
    /// Access size in bytes: 1, 2, 4, or 8 (8 requires a REX.W prefix).
    pub size: u8,
    /// For `MOV r/m32, imm32` instructions (`C7 /0`), the immediate value.
    pub immediate: Option<u32>,
    /// Total instruction length in bytes (prefixes + opcode + ModR/M + SIB + disp + imm).
    ///
    /// WHP does not provide `InstructionLength` for MMIO exits (it's always 0),
    /// so the run loop must use this field to advance RIP.
    pub instruction_len: u8,
}

/// Decode an x86 MMIO instruction from raw bytes.
///
/// # Abstract
///
/// Decodes a memory-mapped I/O (MMIO) instruction that triggered a VM exit.
/// WHP provides the raw instruction bytes but does not decode them. This function
/// identifies the source/destination register, the size of the access, and any
/// immediate values involved.
///
/// # The Hero's Journey
///
/// ```rust
/// use hitz_vmm::mmio_decode::decode_mmio_instruction;
///
/// // Example: `mov dword ptr [rip+disp32], 0x12345678` (write immediate)
/// // C7 05 = opcode and ModR/M
/// // 00 00 00 00 = displacement
/// // 78 56 34 12 = immediate
/// let bytes = [0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12];
/// let decoded = decode_mmio_instruction(&bytes).unwrap();
///
/// assert_eq!(decoded.size, 4); // 32-bit access
/// assert_eq!(decoded.immediate, Some(0x12345678));
/// ```
///
/// # The Fine Print
///
/// Returns `None` if the instruction is not a recognized `MOV` pattern.
/// The `register` field corresponds to x86 standard register indexing
/// (0 = RAX, 1 = RCX, etc.).
#[must_use]
pub fn decode_mmio_instruction(bytes: &[u8]) -> Option<DecodedMmio> {
    if bytes.is_empty() {
        return None;
    }

    let mut pos = 0;
    let (has_operand_size_prefix, rex) = parse_prefixes(bytes, &mut pos)?;

    let rex_r = (rex >> 2) & 1;
    // REX.W (bit 3, `rex & 0x08`) promotes the operand to 64-bit and takes
    // precedence over the 0x66 operand-size prefix.
    let rex_w = ((rex >> 3) & 1) == 1;

    let opcode = *bytes.get(pos)?;
    pos += 1;

    let modrm = *bytes.get(pos)?;
    pos += 1;

    let reg_field = ((modrm >> 3) & 7) | (rex_r << 3);

    parse_sib_and_disp(bytes, modrm, &mut pos)?;

    // -- 5. Opcode-specific decoding --------------------------------------------
    match opcode {
        // MOV r/m, r (write) or MOV r, r/m (read) — 8/16/32/64-bit variants
        0x88..=0x8B => Some(decode_mov_r_rm(
            opcode,
            has_operand_size_prefix,
            rex_w,
            reg_field,
            pos,
        )),
        // MOV r/m, imm
        0xC7 => decode_mov_rm_imm(bytes, has_operand_size_prefix, rex_w, modrm, pos),
        _ => None,
    }
}

fn parse_prefixes(bytes: &[u8], pos: &mut usize) -> Option<(bool, u8)> {
    let mut has_operand_size_prefix = false;
    let mut rex: u8 = 0;

    loop {
        let b = *bytes.get(*pos)?;
        match b {
            0x66 => {
                has_operand_size_prefix = true;
                *pos += 1;
            }
            b @ 0x40..=0x4F => {
                rex = b;
                *pos += 1;
            }
            _ => break,
        }
    }
    Some((has_operand_size_prefix, rex))
}

fn parse_sib_and_disp(bytes: &[u8], modrm: u8, pos: &mut usize) -> Option<()> {
    let has_sib = needs_sib(modrm);

    let sib = if has_sib {
        let s = *bytes.get(*pos)?;
        *pos += 1;
        s
    } else {
        0
    };

    let disp_size = displacement_size(modrm, has_sib, sib);
    if *pos + disp_size > bytes.len() {
        return None;
    }
    *pos += disp_size;

    Some(())
}

/// Compute the operand size in bytes following x86-64 precedence.
///
/// Order matters: a byte-variant opcode is always 1 byte; otherwise `REX.W`
/// promotes to 8 bytes and **overrides** the `0x66` operand-size prefix; a
/// lone `0x66` selects 2 bytes; the default is 4 bytes.
const fn operand_size(is_byte: bool, rex_w: bool, has_operand_size_prefix: bool) -> u8 {
    if is_byte {
        1
    } else if rex_w {
        8
    } else if has_operand_size_prefix {
        2
    } else {
        4
    }
}

#[allow(clippy::cast_possible_truncation)]
const fn decode_mov_r_rm(
    opcode: u8,
    has_operand_size_prefix: bool,
    rex_w: bool,
    reg_field: u8,
    pos: usize,
) -> DecodedMmio {
    // 0x88 / 0x8A are the byte-variant opcodes (low bit clear).
    let size = operand_size(opcode & 1 == 0, rex_w, has_operand_size_prefix);
    DecodedMmio {
        register: reg_field,
        size,
        immediate: None,
        instruction_len: pos as u8,
    }
}

#[allow(clippy::cast_possible_truncation)]
const fn decode_mov_rm_imm(
    bytes: &[u8],
    has_operand_size_prefix: bool,
    rex_w: bool,
    modrm: u8,
    mut pos: usize,
) -> Option<DecodedMmio> {
    // /0 encoding -- reg field must be 0
    if (modrm >> 3) & 7 != 0 {
        return None;
    }

    // The access width follows the standard operand-size precedence (REX.W
    // wins over 0x66). 0xC7 is never a byte-variant opcode.
    let size = operand_size(false, rex_w, has_operand_size_prefix);

    // The encoded immediate itself is at most imm32: a 64-bit store
    // (`C7` with REX.W) still uses a sign-extended imm32, so only 0x66
    // *without* REX.W narrows the immediate to 2 bytes.
    let imm_size = if has_operand_size_prefix && !rex_w {
        2
    } else {
        4
    };
    if pos + imm_size > bytes.len() {
        return None;
    }
    let imm = if imm_size == 2 {
        u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as u32
    } else {
        u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
    };
    pos += imm_size;

    Some(DecodedMmio {
        register: 0, // C7 /0 uses an immediate, not a register source
        size,
        immediate: Some(imm),
        instruction_len: pos as u8,
    })
}

/// Returns `true` if a SIB byte follows the ModR/M byte.
///
/// SIB is present when `r/m == 4` and `mod != 3`.
const fn needs_sib(modrm: u8) -> bool {
    let mod_bits = modrm >> 6;
    let rm = modrm & 7;
    rm == 4 && mod_bits != 3
}

/// Compute the displacement size in bytes from ModR/M and optional SIB.
const fn displacement_size(modrm: u8, has_sib: bool, sib: u8) -> usize {
    let mod_bits = modrm >> 6;
    let rm = modrm & 7;
    match mod_bits {
        0 => {
            if rm == 5 {
                // RIP-relative: disp32
                4
            } else if has_sib && (sib & 7) == 5 {
                // SIB base=5 + disp32
                4
            } else {
                0
            }
        }
        1 => 1, // disp8
        2 => 4, // disp32
        _ => 0, // mod=3 is register-direct (shouldn't appear for MMIO)
    }
}

/// Extract a 64-bit register value from [`StandardRegs`] by x86 register index.
///
/// Index mapping: 0=RAX, 1=RCX, 2=RDX, 3=RBX, 4=RSP, 5=RBP,
/// 6=RSI, 7=RDI, 8–15=R8–R15.
#[must_use]
pub const fn register_value(regs: &StandardRegs, idx: u8) -> u64 {
    match idx {
        0 => regs.rax,
        1 => regs.rcx,
        2 => regs.rdx,
        3 => regs.rbx,
        4 => regs.rsp,
        5 => regs.rbp,
        6 => regs.rsi,
        7 => regs.rdi,
        8 => regs.r8,
        9 => regs.r9,
        10 => regs.r10,
        11 => regs.r11,
        12 => regs.r12,
        13 => regs.r13,
        14 => regs.r14,
        15 => regs.r15,
        _ => 0,
    }
}

/// Set a register value in [`StandardRegs`] by x86 register index.
///
/// Index mapping matches [`register_value`].
pub const fn set_register(regs: &mut StandardRegs, idx: u8, val: u64) {
    match idx {
        0 => regs.rax = val,
        1 => regs.rcx = val,
        2 => regs.rdx = val,
        3 => regs.rbx = val,
        4 => regs.rsp = val,
        5 => regs.rbp = val,
        6 => regs.rsi = val,
        7 => regs.rdi = val,
        8 => regs.r8 = val,
        9 => regs.r9 = val,
        10 => regs.r10 = val,
        11 => regs.r11 = val,
        12 => regs.r12 = val,
        13 => regs.r13 = val,
        14 => regs.r14 = val,
        15 => regs.r15 = val,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    // -- decode_mmio_instruction tests ----------------------------------------

    #[test]
    fn decode_mov_r32_mem_read_rax() {
        // 8B 05 xx xx xx xx = MOV eax, [rip+disp32]
        // ModR/M: 05 => mod=00, reg=000 (RAX), r/m=101 (RIP-relative)
        // Length: opcode(1) + ModR/M(1) + disp32(4) = 6
        let bytes = [0x8B, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0, "register should be RAX (0)");
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 6);
    }

    #[test]
    fn decode_mov_mem_r32_write_ecx() {
        // 89 0D xx xx xx xx = MOV [rip+disp32], ecx
        // ModR/M: 0D => mod=00, reg=001 (ECX), r/m=101 (RIP-relative)
        // Length: opcode(1) + ModR/M(1) + disp32(4) = 6
        let bytes = [0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 1, "register should be ECX (1)");
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 6);
    }

    #[test]
    fn decode_with_rex_prefix_r8d() {
        // 44 8B 05 xx xx xx xx = MOV r8d, [rip+disp32]
        // REX = 0x44 => REX.R = 1
        // ModR/M: 05 => mod=00, reg=000, r/m=101 (RIP-relative)
        // reg = (000 | R<<3) = 8 => R8
        // Length: REX(1) + opcode(1) + ModR/M(1) + disp32(4) = 7
        let bytes = [0x44, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 8, "register should be R8 (8)");
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 7);
    }

    #[test]
    fn decode_8bit_write() {
        // 88 05 xx xx xx xx = MOV [rip+disp32], al
        // ModR/M: 05 => mod=00, reg=000 (AL), r/m=101
        // Length: opcode(1) + ModR/M(1) + disp32(4) = 6
        let bytes = [0x88, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0, "register should be RAX/AL (0)");
        assert_eq!(decoded.size, 1);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 6);
    }

    #[test]
    fn decode_8bit_read() {
        // 8A 05 xx xx xx xx = MOV al, [rip+disp32]
        // Length: opcode(1) + ModR/M(1) + disp32(4) = 6
        let bytes = [0x8A, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 1);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 6);
    }

    #[test]
    fn decode_mov_mem_imm32() {
        // C7 05 dd dd dd dd ii ii ii ii = MOV [rip+disp32], imm32
        // ModR/M: 05 => mod=00, reg=000 (/0), r/m=101 (RIP-relative)
        // disp32 = 4 bytes, then imm32
        // Length: opcode(1) + ModR/M(1) + disp32(4) + imm32(4) = 10
        let bytes = [
            0xC7, 0x05, 0x10, 0x20, 0x30, 0x40, // opcode + ModR/M + disp32
            0x76, 0x69, 0x72, 0x74, // imm32 = 0x74726976 ("virt" LE)
        ];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, Some(0x7472_6976));
        assert_eq!(decoded.instruction_len, 10);
    }

    #[test]
    fn decode_mov_mem_imm32_with_sib() {
        // C7 04 25 00 00 D0 00 ii ii ii ii
        // ModR/M: 04 => mod=00, reg=000 (/0), r/m=100 (SIB follows)
        // SIB: 25 => scale=00, index=100 (none), base=101 (disp32)
        // So: SIB base=5, mod=00 => disp32 follows SIB
        // Length: opcode(1) + ModR/M(1) + SIB(1) + disp32(4) + imm32(4) = 11
        let bytes = [
            0xC7, 0x04, 0x25, // opcode + ModR/M + SIB
            0x00, 0x00, 0x00, 0xD0, // disp32
            0xAB, 0xCD, 0xEF, 0x01, // imm32 = 0x01EFCDAB
        ];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, Some(0x01EF_CDAB));
        assert_eq!(decoded.instruction_len, 11);
    }

    #[test]
    fn decode_with_operand_size_prefix() {
        // 66 89 0D xx xx xx xx = MOV [rip+disp32], cx (16-bit)
        // Length: prefix(1) + opcode(1) + ModR/M(1) + disp32(4) = 7
        let bytes = [0x66, 0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 1, "register should be CX (1)");
        assert_eq!(decoded.size, 2);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 7);
    }

    #[test]
    fn decode_with_operand_size_prefix_read() {
        // 66 8B 05 xx xx xx xx = MOV ax, [rip+disp32] (16-bit)
        // Length: prefix(1) + opcode(1) + ModR/M(1) + disp32(4) = 7
        let bytes = [0x66, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 2);
        assert_eq!(decoded.instruction_len, 7);
    }

    // -- REX.W (64-bit operand) tests -----------------------------------------

    #[test]
    fn decode_no_rex_is_32bit() {
        // 89 0D xx xx xx xx = MOV [rip+disp32], ecx (no REX → 4-byte access)
        let bytes = [0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.size, 4, "no REX.W → 32-bit access");
    }

    #[test]
    fn decode_rexw_write_is_64bit() {
        // 48 89 0D xx xx xx xx = MOV [rip+disp32], rcx (REX.W → 8-byte access)
        // REX = 0x48 => REX.W = 1
        // Length: REX(1) + opcode(1) + ModR/M(1) + disp32(4) = 7
        let bytes = [0x48, 0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 1, "register should be RCX (1)");
        assert_eq!(decoded.size, 8, "REX.W → 64-bit access");
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 7);

        // The decoded 8-byte width must let the write path emit all 64 bits
        // (before this fix, size==4 silently truncated the high dword).
        let value: u64 = 0x1122_3344_5566_7788;
        let size = usize::from(decoded.size);
        let mut data = [0u8; 8];
        data[..size].copy_from_slice(&value.to_le_bytes()[..size]);
        assert_eq!(
            u64::from_le_bytes(data),
            value,
            "8-byte write must preserve the full 64-bit register value"
        );
    }

    #[test]
    fn decode_rexw_read_is_64bit() {
        // 48 8B 05 xx xx xx xx = MOV rax, [rip+disp32] (REX.W → 8-byte access)
        // Length: REX(1) + opcode(1) + ModR/M(1) + disp32(4) = 7
        let bytes = [0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0, "register should be RAX (0)");
        assert_eq!(decoded.size, 8, "REX.W → 64-bit access");
        assert_eq!(decoded.instruction_len, 7);
    }

    #[test]
    fn decode_rexw_byte_variant_stays_1() {
        // 48 88 05 xx xx xx xx = MOV [rip+disp32], al
        // REX.W on a byte-variant opcode (0x88) does not widen the operand.
        let bytes = [0x48, 0x88, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.size, 1, "byte-variant opcode ignores REX.W");
    }

    #[test]
    fn decode_rexw_with_rex_r_r8_is_64bit() {
        // 4C 89 05 xx xx xx xx = MOV [rip+disp32], r8
        // REX = 0x4C => REX.W = 1, REX.R = 1
        // reg = (000 | R<<3) = 8 => R8
        let bytes = [0x4C, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 8, "register should be R8 (8)");
        assert_eq!(decoded.size, 8, "REX.W → 64-bit access even with REX.R");
    }

    #[test]
    fn decode_rexw_overrides_operand_size_prefix() {
        // Both 0x66 (→2) and REX.W (→8) are present; REX.W must win.
        // parse_prefixes accepts the two prefixes in either order, so test both.
        // 66 48 89 0D xx xx xx xx
        let prefix_first = [0x66, 0x48, 0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&prefix_first).expect("should decode");
        assert_eq!(
            decoded.size, 8,
            "REX.W must override the 0x66 operand-size prefix"
        );
        assert_eq!(decoded.instruction_len, 8);

        // 48 66 89 0D xx xx xx xx  (REX byte first)
        let rex_first = [0x48, 0x66, 0x89, 0x0D, 0x00, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&rex_first).expect("should decode");
        assert_eq!(
            decoded.size, 8,
            "REX.W must override 0x66 regardless of prefix order"
        );
    }

    #[test]
    fn decode_c7_rexw_imm_is_64bit_access_with_imm32() {
        // 48 C7 05 dd dd dd dd ii ii ii ii = MOV qword ptr [rip+disp32], imm32
        // REX.W → 8-byte access, but the encoded immediate is still a
        // (sign-extended) imm32, so imm_size stays 4.
        // Length: REX(1) + opcode(1) + ModR/M(1) + disp32(4) + imm32(4) = 11
        let bytes = [
            0x48, 0xC7, 0x05, 0x10, 0x20, 0x30, 0x40, // REX.W + opcode + ModR/M + disp32
            0x78, 0x56, 0x34, 0x12, // imm32 = 0x12345678
        ];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 8, "REX.W → 64-bit store");
        assert_eq!(decoded.immediate, Some(0x1234_5678));
        assert_eq!(decoded.instruction_len, 11);
    }

    #[test]
    fn decode_c7_rexw_overrides_operand_size_prefix() {
        // 66 48 C7 05 dd dd dd dd ii ii ii ii
        // 0x66 alone would make this a 16-bit store with a 2-byte immediate;
        // REX.W overrides it → 8-byte access with a 4-byte immediate.
        let bytes = [
            0x66, 0x48, 0xC7, 0x05, 0x10, 0x20, 0x30,
            0x40, // prefixes + opcode + ModR/M + disp32
            0x78, 0x56, 0x34, 0x12, // imm32 (must NOT be read as imm16)
        ];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.size, 8, "REX.W overrides 0x66 for C7 too");
        assert_eq!(
            decoded.immediate,
            Some(0x1234_5678),
            "REX.W keeps the immediate at imm32, not imm16"
        );
        assert_eq!(decoded.instruction_len, 12);
    }

    #[test]
    fn operand_size_precedence() {
        // Byte opcode always wins.
        assert_eq!(operand_size(true, true, true), 1);
        assert_eq!(operand_size(true, false, false), 1);
        // REX.W beats the 0x66 prefix.
        assert_eq!(operand_size(false, true, true), 8);
        assert_eq!(operand_size(false, true, false), 8);
        // 0x66 alone.
        assert_eq!(operand_size(false, false, true), 2);
        // Default.
        assert_eq!(operand_size(false, false, false), 4);
    }

    #[test]
    fn decode_empty_returns_none() {
        assert!(decode_mmio_instruction(&[]).is_none());
    }

    #[test]
    fn decode_unknown_opcode_returns_none() {
        // FF is not a recognized MOV opcode for MMIO.
        assert!(decode_mmio_instruction(&[0xFF, 0x00]).is_none());
    }

    #[test]
    fn decode_c7_non_zero_reg_returns_none() {
        // C7 with reg=1 (/1) is not MOV r/m, imm32
        // ModR/M: 0D => mod=00, reg=001, r/m=101
        let bytes = [0xC7, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(decode_mmio_instruction(&bytes).is_none());
    }

    #[test]
    fn decode_mod10_disp32() {
        // 89 8D dd dd dd dd = MOV [rbp+disp32], ecx
        // ModR/M: 8D => mod=10, reg=001 (ECX), r/m=101 (RBP)
        // Length: opcode(1) + ModR/M(1) + disp32(4) = 6
        let bytes = [0x89, 0x8D, 0x00, 0x10, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 1, "register should be ECX (1)");
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.instruction_len, 6);
    }

    #[test]
    fn decode_simple_mov_no_displacement() {
        // 8B 03 = MOV eax, [rbx] — the exact instruction from phase3 magic read test
        // ModR/M: 03 => mod=00, reg=000 (EAX), r/m=011 (RBX) — no SIB, no disp
        // Length: opcode(1) + ModR/M(1) = 2
        let bytes = [0x8B, 0x03];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0, "register should be RAX (0)");
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, None);
        assert_eq!(decoded.instruction_len, 2);
    }

    #[test]
    fn decode_mov_disp8() {
        // 8B 43 70 = MOV eax, [rbx+0x70]
        // ModR/M: 43 => mod=01, reg=000 (EAX), r/m=011 (RBX) — disp8 follows
        // Length: opcode(1) + ModR/M(1) + disp8(1) = 3
        let bytes = [0x8B, 0x43, 0x70];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.instruction_len, 3);
    }

    #[test]
    fn decode_c7_with_disp8() {
        // C7 43 70 vv vv vv vv = MOV [rbx+0x70], imm32
        // ModR/M: 43 => mod=01, reg=000 (/0), r/m=011 (RBX) — disp8 follows
        // Length: opcode(1) + ModR/M(1) + disp8(1) + imm32(4) = 7
        let bytes = [0xC7, 0x43, 0x70, 0x01, 0x00, 0x00, 0x00];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 4);
        assert_eq!(decoded.immediate, Some(1));
        assert_eq!(decoded.instruction_len, 7);
    }

    // -- register_value / set_register tests ----------------------------------

    #[test]
    fn register_value_all_indices() {
        let regs = StandardRegs {
            rax: 100,
            rcx: 101,
            rdx: 102,
            rbx: 103,
            rsp: 104,
            rbp: 105,
            rsi: 106,
            rdi: 107,
            r8: 108,
            r9: 109,
            r10: 110,
            r11: 111,
            r12: 112,
            r13: 113,
            r14: 114,
            r15: 115,
            ..Default::default()
        };

        for i in 0..16u8 {
            assert_eq!(
                register_value(&regs, i),
                u64::from(i) + 100,
                "mismatch for register index {i}"
            );
        }

        // Out of range returns 0.
        assert_eq!(register_value(&regs, 16), 0);
    }

    #[test]
    fn set_register_roundtrip() {
        let mut regs = StandardRegs::default();
        for i in 0..16u8 {
            set_register(&mut regs, i, u64::from(i) * 1000);
        }
        for i in 0..16u8 {
            assert_eq!(
                register_value(&regs, i),
                u64::from(i) * 1000,
                "roundtrip mismatch for index {i}"
            );
        }
    }

    #[test]
    fn set_register_out_of_range_is_noop() {
        let mut regs = StandardRegs::default();
        set_register(&mut regs, 99, 42);
        // Nothing should have changed.
        for i in 0..16u8 {
            assert_eq!(register_value(&regs, i), 0);
        }
    }

    #[test]
    fn test_decode_mov_imm16_with_prefix() {
        // 66 C7 05 10 20 30 40 76 69
        // 66 = operand size prefix
        // C7 = MOV r/m, imm
        // 05 = mod=00, reg=000 (/0), r/m=101 (disp32)
        // 10 20 30 40 = disp32
        // 76 69 = imm16
        // Length: prefix(1) + opcode(1) + ModR/M(1) + disp32(4) + imm16(2) = 9
        let bytes = [0x66, 0xC7, 0x05, 0x10, 0x20, 0x30, 0x40, 0x76, 0x69];
        let decoded = decode_mmio_instruction(&bytes).expect("should decode");
        assert_eq!(decoded.register, 0);
        assert_eq!(decoded.size, 2);
        assert_eq!(decoded.immediate, Some(0x6976));
        assert_eq!(decoded.instruction_len, 9);
    }

    #[test]
    fn test_decode_displacement_out_of_bounds() {
        // 8B 05 00 00
        // Expects 4 bytes of displacement, only 2 provided
        let bytes = [0x8B, 0x05, 0x00, 0x00];
        assert!(decode_mmio_instruction(&bytes).is_none());
    }

    #[test]
    fn test_decode_immediate_out_of_bounds() {
        // C7 05 00 00 00 00 01
        // Expects 4 bytes of immediate, only 1 provided
        let bytes = [0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x01];
        assert!(decode_mmio_instruction(&bytes).is_none());
    }

    #[test]
    fn test_decode_sib_out_of_bounds() {
        // 8B 04
        // 04 = mod=00, reg=000, r/m=100 (SIB)
        // Expects SIB byte, none provided
        let bytes = [0x8B, 0x04];
        assert!(decode_mmio_instruction(&bytes).is_none());
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn havoc_fuzz_decode_mmio_instruction(bytes in proptest::collection::vec(any::<u8>(), 0..15)) {
            let _ = decode_mmio_instruction(&bytes);
        }
    }
}
