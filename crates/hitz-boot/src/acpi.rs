#![allow(clippy::unwrap_used, clippy::expect_used)]
//! ACPI table construction for SMP boot.
//!
//! # Abstract
//!
//! Builds RSDP, XSDT, and MADT (Multiple APIC Description Table) so the
//! Linux kernel can discover multiple vCPUs. Tables are placed in the BIOS
//! read-only region (`0x000E_xxxx`) which the E820 map marks as reserved.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_boot::{build_rsdp, build_xsdt, build_madt};
//!
//! let rsdp = build_rsdp();
//! let xsdt = build_xsdt();
//! let madt = build_madt(4).expect("failed to build MADT for 4 CPUs");
//!
//! assert_eq!(&rsdp[0..8], b"RSD PTR ");
//! assert_eq!(&xsdt[0..4], b"XSDT");
//! assert_eq!(&madt[0..4], b"APIC");
//! ```

use crate::error::BootError;

// ---- GPA constants --------------------------------------------------------

/// Guest physical address of the RSDP structure (36 bytes, ACPI 2.0+).
pub const RSDP_GPA: u64 = 0x000E_0000;

/// Guest physical address of the XSDT (Extended System Description Table).
pub const XSDT_GPA: u64 = 0x000E_1000;

/// Guest physical address of the MADT (Multiple APIC Description Table).
pub const MADT_GPA: u64 = 0x000E_2000;

// ---- Local APIC constants -------------------------------------------------

/// Standard x86 Local APIC base address.
const LOCAL_APIC_ADDRESS: u32 = 0xFEE0_0000;

/// MADT Local APIC entry type.
const MADT_TYPE_LOCAL_APIC: u8 = 0;

/// MADT Local APIC entry length in bytes.
const MADT_LAPIC_ENTRY_LEN: u8 = 8;

/// MADT flags: `PCAT_COMPAT` -- system has a dual-8259 setup.
const MADT_FLAGS_PCAT_COMPAT: u32 = 1;

/// LAPIC flags: processor is enabled.
const LAPIC_FLAGS_ENABLED: u32 = 1;

/// Maximum number of CPUs supported (ACPI processor UID is a u8).
const MAX_CPUS: u32 = 255;

// ---- SDT header layout constants ------------------------------------------

/// Size of an ACPI SDT header in bytes.
const SDT_HEADER_LEN: usize = 36;

/// OEM ID used in all Hitz ACPI tables (6 bytes, padded with spaces).
const OEM_ID: &[u8; 6] = b"HITZ  ";

/// OEM Table ID used in SDT headers (8 bytes, padded with spaces).
const OEM_TABLE_ID: &[u8; 8] = b"HITZVM  ";

/// OEM Revision for SDT headers.
const OEM_REVISION: u32 = 1;

/// Creator ID for SDT headers (4 bytes).
const CREATOR_ID: &[u8; 4] = b"HITZ";

/// Creator Revision for SDT headers.
const CREATOR_REVISION: u32 = 1;

// ---- Checksum helper ------------------------------------------------------

/// Compute the ACPI checksum byte such that the sum of all bytes (including
/// the checksum) is zero mod 256.
fn acpi_checksum(data: &[u8]) -> u8 {
    let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    0u8.wrapping_sub(sum)
}

// ---- RSDP -----------------------------------------------------------------

/// Build an ACPI 2.0 RSDP (Root System Description Pointer).
///
/// Returns a 36-byte array with:
/// - Signature `"RSD PTR "` (bytes 0..8)
/// - Checksum covering bytes 0..20 (legacy, byte 8)
/// - OEM ID `"HITZ  "` (bytes 9..15)
/// - Revision 2 (byte 15)
/// - XSDT physical address (bytes 24..32)
/// - Extended checksum covering all 36 bytes (byte 32)
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::build_rsdp;
///
/// let rsdp = build_rsdp();
/// assert_eq!(&rsdp[0..8], b"RSD PTR ");
/// assert_eq!(rsdp[15], 2); // ACPI 2.0+
/// ```
/// Builds the ACPI Root System Description Pointer (RSDP).
///
/// # Abstract
/// Constructs the RSDP structure which is the entry point for the OS
/// to discover ACPI tables. It points to the XSDT.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_boot::build_rsdp;
///
/// let rsdp = build_rsdp();
/// assert_eq!(&rsdp[0..8], b"RSD PTR ");
/// ```
#[must_use]
pub fn build_rsdp() -> [u8; 36] {
    let mut rsdp = [0u8; 36];

    // Signature (bytes 0..8).
    rsdp[0..8].copy_from_slice(b"RSD PTR ");

    // OEM ID (bytes 9..15).
    rsdp[9..15].copy_from_slice(OEM_ID);

    // Revision (byte 15) — 2 for ACPI 2.0+.
    rsdp[15] = 2;

    // RsdtAddress (bytes 16..20) — unused for XSDT, leave as zero.

    // Length (bytes 20..24) — 36 for ACPI 2.0 RSDP.
    rsdp[20..24].copy_from_slice(&36u32.to_le_bytes());

    // XsdtAddress (bytes 24..32).
    rsdp[24..32].copy_from_slice(&XSDT_GPA.to_le_bytes());

    // Legacy checksum (byte 8) covers bytes 0..20.
    rsdp[8] = 0;
    rsdp[8] = acpi_checksum(&rsdp[0..20]);

    // Extended checksum (byte 32) covers all 36 bytes.
    rsdp[32] = 0;
    rsdp[32] = acpi_checksum(&rsdp);

    rsdp
}

// ---- SDT Header -----------------------------------------------------------

/// Build a generic ACPI SDT header.
///
/// The caller is responsible for setting the checksum (byte 9) after the
/// full table (header + body) is assembled.
fn build_sdt_header(signature: [u8; 4], total_length: u32) -> [u8; SDT_HEADER_LEN] {
    let mut hdr = [0u8; SDT_HEADER_LEN];

    // Signature (bytes 0..4).
    hdr[0..4].copy_from_slice(&signature);

    // Length (bytes 4..8) — total table length including header.
    hdr[4..8].copy_from_slice(&total_length.to_le_bytes());

    // Revision (byte 8).
    hdr[8] = 1;

    // Checksum (byte 9) — caller must fill in after building the full table.

    // OEM ID (bytes 10..16).
    hdr[10..16].copy_from_slice(OEM_ID);

    // OEM Table ID (bytes 16..24).
    hdr[16..24].copy_from_slice(OEM_TABLE_ID);

    // OEM Revision (bytes 24..28).
    hdr[24..28].copy_from_slice(&OEM_REVISION.to_le_bytes());

    // Creator ID (bytes 28..32).
    hdr[28..32].copy_from_slice(CREATOR_ID);

    // Creator Revision (bytes 32..36).
    hdr[32..36].copy_from_slice(&CREATOR_REVISION.to_le_bytes());

    hdr
}

// ---- XSDT -----------------------------------------------------------------

/// Build an XSDT (Extended System Description Table) pointing to the MADT.
///
/// Returns a `Vec<u8>` containing the 36-byte SDT header followed by a
/// single 8-byte pointer to [`MADT_GPA`]. The checksum at byte 9 is valid.
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::build_xsdt;
///
/// let xsdt = build_xsdt();
/// assert_eq!(&xsdt[0..4], b"XSDT");
/// ```
#[must_use]
#[allow(clippy::cast_possible_truncation)] // total_len is always 44
pub fn build_xsdt() -> Vec<u8> {
    let total_len = SDT_HEADER_LEN + 8; // header + one 64-bit pointer
    let hdr = build_sdt_header(*b"XSDT", total_len as u32);

    let mut xsdt = Vec::with_capacity(total_len);
    xsdt.extend_from_slice(&hdr);
    xsdt.extend_from_slice(&MADT_GPA.to_le_bytes());

    // Checksum (byte 9).
    xsdt[9] = 0;
    xsdt[9] = acpi_checksum(&xsdt);

    xsdt
}

// ---- MADT -----------------------------------------------------------------

/// A Local APIC entry in the MADT (type 0, 8 bytes).
struct MadtLapicEntry {
    /// Entry type (always 0 for Local APIC).
    type_: u8,
    /// Entry length (always 8).
    length: u8,
    /// ACPI processor UID.
    acpi_processor_id: u8,
    /// Local APIC ID.
    apic_id: u8,
    /// Flags (bit 0 = enabled).
    flags: u32,
}

impl MadtLapicEntry {
    /// Serialize this entry to an 8-byte array.
    fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = self.type_;
        buf[1] = self.length;
        buf[2] = self.acpi_processor_id;
        buf[3] = self.apic_id;
        buf[4..8].copy_from_slice(&self.flags.to_le_bytes());
        buf
    }
}

/// Build a MADT (Multiple APIC Description Table) for `cpu_count` processors.
///
/// The table contains:
/// - 36-byte SDT header with signature `"APIC"`
/// - 4-byte Local APIC Address (`0xFEE0_0000`)
/// - 4-byte Flags (PCAT\_COMPAT)
/// - N Local APIC entries (8 bytes each)
///
/// Total length: `36 + 4 + 4 + cpu_count * 8`
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::build_madt;
///
/// let madt = build_madt(2).expect("should build for 2 CPUs");
/// assert_eq!(&madt[0..4], b"APIC");
/// assert_eq!(madt.len(), 36 + 4 + 4 + 2 * 8);
/// ```
///
/// ```compile_fail
/// use hitz_boot::build_madt;
/// // Fails at compile-time if used improperly (example syntax check)
/// let madt = build_madt("two");
/// ```
///
/// # Errors
///
/// Returns [`BootError::InvalidBootParams`] if `cpu_count` is 0 or exceeds 255.
/// Builds the ACPI Multiple APIC Description Table (MADT).
///
/// # Abstract
/// Constructs the MADT which tells the guest OS about the available
/// processors (vCPUs) and the interrupt controller (APIC) configuration.
///
/// # The Hero's Journey
/// ```rust
/// use hitz_boot::build_madt;
///
/// let madt = build_madt(4).unwrap();
/// assert_eq!(&madt[0..4], b"APIC");
/// ```
#[allow(clippy::cast_possible_truncation)]
pub fn build_madt(cpu_count: u32) -> Result<Vec<u8>, BootError> {
    if cpu_count == 0 {
        return Err(BootError::InvalidBootParams(
            "cpu_count must be at least 1".to_string(),
        ));
    }
    if cpu_count > MAX_CPUS {
        return Err(BootError::InvalidBootParams(format!(
            "cpu_count ({cpu_count}) exceeds maximum of {MAX_CPUS}"
        )));
    }

    let body_len = 4 + 4 + cpu_count as usize * 8; // LAPIC addr + flags + entries
    let total_len = SDT_HEADER_LEN + body_len;
    let hdr = build_sdt_header(*b"APIC", total_len as u32);

    let mut madt = Vec::with_capacity(total_len);
    madt.extend_from_slice(&hdr);

    // Local APIC Address (4 bytes).
    madt.extend_from_slice(&LOCAL_APIC_ADDRESS.to_le_bytes());

    // Flags (4 bytes).
    madt.extend_from_slice(&MADT_FLAGS_PCAT_COMPAT.to_le_bytes());

    // Local APIC entries.
    for i in 0..cpu_count {
        let entry = MadtLapicEntry {
            type_: MADT_TYPE_LOCAL_APIC,
            length: MADT_LAPIC_ENTRY_LEN,
            acpi_processor_id: i as u8,
            apic_id: i as u8,
            flags: LAPIC_FLAGS_ENABLED,
        };
        madt.extend_from_slice(&entry.to_bytes());
    }

    // Checksum (byte 9).
    madt[9] = 0;
    madt[9] = acpi_checksum(&madt);

    Ok(madt)
}

// ---- Tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the sum of all bytes in `data` is 0 mod 256.
    fn verify_checksum(data: &[u8]) {
        let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        assert_eq!(sum, 0, "checksum invalid: sum = {sum}");
    }

    #[test]
    fn rsdp_checksums_valid() {
        let rsdp = build_rsdp();
        // Legacy checksum covers bytes 0..20.
        verify_checksum(&rsdp[0..20]);
        // Extended checksum covers all 36 bytes.
        verify_checksum(&rsdp);
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
        let xsdt_addr =
            u64::from_le_bytes(rsdp[24..32].try_into().expect("slice should be 8 bytes"));
        assert_eq!(xsdt_addr, XSDT_GPA);
    }

    #[test]
    fn xsdt_checksum_valid() {
        let xsdt = build_xsdt();
        verify_checksum(&xsdt);
    }

    #[test]
    fn xsdt_contains_madt_pointer() {
        let xsdt = build_xsdt();
        let madt_addr =
            u64::from_le_bytes(xsdt[36..44].try_into().expect("slice should be 8 bytes"));
        assert_eq!(madt_addr, MADT_GPA);
    }

    #[test]
    fn madt_1_cpu() {
        let madt = build_madt(1).expect("1 CPU should succeed");

        // Total length: 36 header + 4 LAPIC addr + 4 flags + 1*8 entries = 52
        assert_eq!(madt.len(), 52);

        // Checksum.
        verify_checksum(&madt);

        // Signature.
        assert_eq!(&madt[0..4], b"APIC");

        // Length field matches actual length.
        let length = u32::from_le_bytes(madt[4..8].try_into().expect("slice should be 4 bytes"));
        assert_eq!(length, 52);

        // Local APIC address.
        let lapic_addr =
            u32::from_le_bytes(madt[36..40].try_into().expect("slice should be 4 bytes"));
        assert_eq!(lapic_addr, LOCAL_APIC_ADDRESS);

        // First LAPIC entry (starts at offset 44).
        assert_eq!(madt[44], MADT_TYPE_LOCAL_APIC); // type
        assert_eq!(madt[45], MADT_LAPIC_ENTRY_LEN); // length
        assert_eq!(madt[46], 0); // acpi_processor_id
        assert_eq!(madt[47], 0); // apic_id
        let flags = u32::from_le_bytes(madt[48..52].try_into().expect("slice should be 4 bytes"));
        assert_eq!(flags, LAPIC_FLAGS_ENABLED);
    }

    #[test]
    fn madt_4_cpus() {
        let madt = build_madt(4).expect("4 CPUs should succeed");

        // Total length: 36 + 4 + 4 + 4*8 = 76
        assert_eq!(madt.len(), 76);

        // Checksum.
        verify_checksum(&madt);

        // 4th LAPIC entry (index 3) starts at offset 44 + 3*8 = 68.
        let entry_offset = 44 + 3 * 8;
        assert_eq!(madt[entry_offset], MADT_TYPE_LOCAL_APIC); // type
        assert_eq!(madt[entry_offset + 1], MADT_LAPIC_ENTRY_LEN); // length
        assert_eq!(madt[entry_offset + 2], 3); // acpi_processor_id
        assert_eq!(madt[entry_offset + 3], 3); // apic_id
        let flags = u32::from_le_bytes(
            madt[entry_offset + 4..entry_offset + 8]
                .try_into()
                .expect("slice should be 4 bytes"),
        );
        assert_eq!(flags, LAPIC_FLAGS_ENABLED);
    }

    #[test]
    fn madt_0_cpus_fails() {
        let result = build_madt(0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BootError::InvalidBootParams(_)),
            "expected InvalidBootParams, got {err:?}"
        );
    }

    #[test]
    fn madt_256_cpus_fails() {
        let result = build_madt(256);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BootError::InvalidBootParams(_)),
            "expected InvalidBootParams, got {err:?}"
        );
    }
}
