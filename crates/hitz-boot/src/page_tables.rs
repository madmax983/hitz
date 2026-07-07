#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::identity_op
)]
//! Identity-mapped x86-64 page table generation.
//!
//! # Abstract
//!
//! Builds 4-level page tables using 2 MiB huge pages. The output is a
//! sequence of [`MemWrite`] chunks that the caller writes into guest
//! physical memory — `hitz-boot` never touches guest RAM directly.
//!
//! ## Examples
//!
//! ```rust
//! use hitz_boot::build_page_tables;
//! use hitz_hal::Gpa;
//!
//! // Build page tables to map 1 GiB of RAM
//! let (cr3_gpa, writes) = build_page_tables(1).unwrap();
//!
//! assert_eq!(cr3_gpa, Gpa::new(0x8000)); // The PML4 base
//! assert_eq!(writes.len(), 3); // PML4, PDPT, and 1 PD table
//! ```

use hitz_hal::Gpa;

use crate::error::BootError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A chunk of data to write into guest memory.
#[derive(Debug, Clone)]
pub struct MemWrite {
    /// Guest physical address where the data should be written.
    pub gpa: Gpa,
    /// Raw bytes to write at `gpa`.
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Guest physical address of the PML4 table.
pub const PML4_GPA: u64 = 0x8000;
/// Guest physical address of the PDPT table.
pub const PDPT_GPA: u64 = 0x9000;
/// Guest physical address of the first Page Directory table.
pub const PD_BASE_GPA: u64 = 0xA000;

/// Page is present in memory.
const PAGE_PRESENT: u64 = 1 << 0;
/// Page is read/write.
const PAGE_RW: u64 = 1 << 1;
/// PS bit — marks a 2 MiB huge page in a PD entry.
const PAGE_SIZE_FLAG: u64 = 1 << 7;

/// Flags for a table entry that points to a child table (PML4 -> PDPT, PDPT -> PD).
const TABLE_FLAGS: u64 = PAGE_PRESENT | PAGE_RW; // 0x3
/// Flags for a 2 MiB huge page leaf entry in a PD table.
const HUGE_PAGE_FLAGS: u64 = PAGE_PRESENT | PAGE_RW | PAGE_SIZE_FLAG; // 0x83

/// Size of a single page table (4 KiB).
const PAGE_SIZE: usize = 4096;
/// Number of 8-byte entries per page table.
const ENTRIES_PER_TABLE: usize = 512;
/// Size of a 2 MiB huge page in bytes.
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
/// Size of 1 GiB in bytes.
const ONE_GIB: u64 = 1024 * 1024 * 1024;
/// Maximum GiB count supported (PDPT has 512 entries, each mapping 1 GiB).
const MAX_GIB: u32 = 512;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build identity-mapped page tables for `gib_count` GiB of physical memory.
///
/// Returns `(pml4_gpa, writes)` where `pml4_gpa` is the address to load into
/// CR3 and `writes` is the set of page-table pages to write into guest RAM.
///
/// Uses 2 MiB huge pages so only three levels are needed:
/// PML4 -> PDPT -> PD (with PS bit set).
///
/// ## Examples
///
/// ```rust
/// use hitz_boot::build_page_tables;
///
/// // Create page tables for a guest with 4 GiB of memory.
/// let (cr3, writes) = build_page_tables(4).expect("should succeed");
/// assert_eq!(writes.len(), 6); // 1 PML4 + 1 PDPT + 4 PDs
/// ```
///
/// ```compile_fail
/// use hitz_boot::build_page_tables;
/// build_page_tables(-1); // Compiler error: must be u32
/// ```
///
/// # Errors
///
/// Returns [`BootError::InvalidPageTableConfig`] if `gib_count` is zero or
/// exceeds 512 (the maximum a single PDPT can map).
pub fn build_page_tables(gib_count: u32) -> Result<(Gpa, Vec<MemWrite>), BootError> {
    if gib_count == 0 || gib_count > MAX_GIB {
        return Err(BootError::InvalidPageTableConfig(format!(
            "gib_count must be in 1..={MAX_GIB}, got {gib_count}"
        )));
    }

    // Total writes: 1 PML4 + 1 PDPT + gib_count PD tables.
    let total_writes = 2 + gib_count as usize;
    let mut writes = Vec::with_capacity(total_writes);

    // -- PML4 (one entry pointing at the PDPT) ----------------------------
    let mut pml4 = vec![0u8; PAGE_SIZE];
    write_entry(&mut pml4, 0, PDPT_GPA | TABLE_FLAGS);
    writes.push(MemWrite {
        gpa: Gpa::new(PML4_GPA),
        data: pml4,
    });

    // -- PDPT (one entry per GiB, each pointing at a PD table) ------------
    let mut pdpt = vec![0u8; PAGE_SIZE];
    for i in 0..gib_count {
        let pd_gpa = PD_BASE_GPA + u64::from(i) * PAGE_SIZE as u64;
        write_entry(&mut pdpt, i as usize, pd_gpa | TABLE_FLAGS);
    }
    writes.push(MemWrite {
        gpa: Gpa::new(PDPT_GPA),
        data: pdpt,
    });

    // -- PD tables (512 x 2 MiB huge-page entries each) -------------------
    for i in 0..gib_count {
        let mut pd = vec![0u8; PAGE_SIZE];
        for j in 0..ENTRIES_PER_TABLE {
            let phys_addr = u64::from(i) * ONE_GIB + j as u64 * HUGE_PAGE_SIZE;
            write_entry(&mut pd, j, phys_addr | HUGE_PAGE_FLAGS);
        }
        writes.push(MemWrite {
            gpa: Gpa::new(PD_BASE_GPA + u64::from(i) * PAGE_SIZE as u64),
            data: pd,
        });
    }

    Ok((Gpa::new(PML4_GPA), writes))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a 64-bit little-endian value into the `index`-th slot of a page table.
fn write_entry(table: &mut [u8], index: usize, value: u64) {
    let offset = index * 8;
    table[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Read a 64-bit little-endian value from the `index`-th slot of a page table.
#[cfg(test)]
fn read_entry(table: &[u8], index: usize) -> u64 {
    let offset = index * 8;
    u64::from_le_bytes(table[offset..offset + 8].try_into().expect("8 bytes"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_4gib_page_tables() {
        let (cr3, writes) = build_page_tables(4).expect("4 GiB should succeed");

        // 1 PML4 + 1 PDPT + 4 PD = 6 writes
        assert_eq!(writes.len(), 6);

        // CR3 should be the PML4 address.
        assert_eq!(cr3, Gpa::new(PML4_GPA));

        // -- PML4 --
        let pml4 = &writes[0];
        assert_eq!(pml4.gpa, Gpa::new(PML4_GPA));
        assert_eq!(pml4.data.len(), PAGE_SIZE);
        assert_eq!(read_entry(&pml4.data, 0), PDPT_GPA | TABLE_FLAGS);
        // Remaining PML4 entries should be zero.
        for i in 1..ENTRIES_PER_TABLE {
            assert_eq!(read_entry(&pml4.data, i), 0, "PML4[{i}] should be zero");
        }

        // -- PDPT --
        let pdpt = &writes[1];
        assert_eq!(pdpt.gpa, Gpa::new(PDPT_GPA));
        assert_eq!(pdpt.data.len(), PAGE_SIZE);
        for i in 0..4u64 {
            let expected = (PD_BASE_GPA + i * PAGE_SIZE as u64) | TABLE_FLAGS;
            assert_eq!(
                read_entry(&pdpt.data, i as usize),
                expected,
                "PDPT[{i}] mismatch"
            );
        }
        // Remaining PDPT entries should be zero.
        for i in 4..ENTRIES_PER_TABLE {
            assert_eq!(read_entry(&pdpt.data, i), 0, "PDPT[{i}] should be zero");
        }

        // -- PD[0]: maps 0x0000_0000 .. 0x3FFF_FFFF --
        let pd0 = &writes[2];
        assert_eq!(pd0.gpa, Gpa::new(PD_BASE_GPA));
        assert_eq!(pd0.data.len(), PAGE_SIZE);
        assert_eq!(read_entry(&pd0.data, 0), 0x0 | HUGE_PAGE_FLAGS);
        assert_eq!(read_entry(&pd0.data, 1), HUGE_PAGE_SIZE | HUGE_PAGE_FLAGS);
        assert_eq!(
            read_entry(&pd0.data, 511),
            (511 * HUGE_PAGE_SIZE) | HUGE_PAGE_FLAGS
        );

        // -- PD[1]: maps 0x4000_0000 .. 0x7FFF_FFFF --
        let pd1 = &writes[3];
        assert_eq!(pd1.gpa, Gpa::new(PD_BASE_GPA + PAGE_SIZE as u64));
        assert_eq!(read_entry(&pd1.data, 0), ONE_GIB | HUGE_PAGE_FLAGS);

        // -- PD[3]: maps 0xC000_0000 .. 0xFFFF_FFFF --
        let pd3 = &writes[5];
        let last_huge_page = 4 * ONE_GIB - HUGE_PAGE_SIZE;
        assert_eq!(read_entry(&pd3.data, 511), last_huge_page | HUGE_PAGE_FLAGS);
    }

    #[test]
    fn test_1gib_page_tables() {
        let (cr3, writes) = build_page_tables(1).expect("1 GiB should succeed");

        // 1 PML4 + 1 PDPT + 1 PD = 3 writes
        assert_eq!(writes.len(), 3);
        assert_eq!(cr3, Gpa::new(PML4_GPA));

        // PDPT should have exactly one entry.
        let pdpt = &writes[1];
        assert_eq!(read_entry(&pdpt.data, 0), PD_BASE_GPA | TABLE_FLAGS);
        assert_eq!(read_entry(&pdpt.data, 1), 0);
    }

    #[test]
    fn test_pml4_gpa() {
        let (cr3, _) = build_page_tables(1).expect("should succeed");
        assert_eq!(cr3, Gpa::new(0x8000));
    }

    #[test]
    fn test_zero_gib_error() {
        let result = build_page_tables(0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BootError::InvalidPageTableConfig(_)),
            "expected InvalidPageTableConfig, got {err:?}"
        );
    }

    #[test]
    fn test_exceeds_max_gib_error() {
        let result = build_page_tables(513);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BootError::InvalidPageTableConfig(_)
        ));
    }

    #[test]
    fn test_max_gib_succeeds() {
        let (_, writes) = build_page_tables(512).expect("512 GiB should succeed");
        // 1 PML4 + 1 PDPT + 512 PD = 514 writes
        assert_eq!(writes.len(), 514);
    }

    #[test]
    fn test_page_sizes() {
        let (_, writes) = build_page_tables(4).expect("should succeed");
        for (i, w) in writes.iter().enumerate() {
            assert_eq!(
                w.data.len(),
                PAGE_SIZE,
                "MemWrite[{i}] data should be exactly 4096 bytes"
            );
        }
    }

    #[test]
    fn test_identity_mapping_correctness() {
        // Verify that every 2 MiB page maps to itself (identity mapping).
        let (_, writes) = build_page_tables(2).expect("should succeed");

        // PD[0] covers 0..1 GiB
        let pd0 = &writes[2];
        for j in 0..ENTRIES_PER_TABLE {
            let expected_phys = j as u64 * HUGE_PAGE_SIZE;
            let entry = read_entry(&pd0.data, j);
            assert_eq!(
                entry & !0xFFF,
                expected_phys,
                "PD[0][{j}] physical address mismatch"
            );
            assert_eq!(entry & 0xFFF, HUGE_PAGE_FLAGS, "PD[0][{j}] flags mismatch");
        }

        // PD[1] covers 1 GiB..2 GiB
        let pd1 = &writes[3];
        for j in 0..ENTRIES_PER_TABLE {
            let expected_phys = ONE_GIB + j as u64 * HUGE_PAGE_SIZE;
            let entry = read_entry(&pd1.data, j);
            assert_eq!(
                entry & !0xFFF,
                expected_phys,
                "PD[1][{j}] physical address mismatch"
            );
        }
    }

    #[test]
    fn test_gpa_ordering() {
        // Verify writes are in ascending GPA order.
        let (_, writes) = build_page_tables(4).expect("should succeed");
        for window in writes.windows(2) {
            assert!(
                window[0].gpa < window[1].gpa,
                "GPAs should be in ascending order: {:?} >= {:?}",
                window[0].gpa,
                window[1].gpa
            );
        }
    }
}
