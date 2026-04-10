//! Guest physical memory backed by `VirtualAlloc` on Windows.
//!
//! # Abstract
//! Because the upstream `vm-memory` crate does not compile on Windows
//! (libc `read`/`write` signature mismatches), we roll our own minimal
//! guest memory abstraction using the Win32 `VirtualAlloc` / `VirtualFree`
//! APIs directly.
//!
//! This module translates between Guest Physical Addresses (GPA) and Host
//! Virtual Addresses (HVA).
//!
//! # The Hero's Journey
//! ```
//! # use hitz_vmm::memory::GuestMemory;
//! # use hitz_hal::Gpa;
//! // 1. Create the empty memory manager.
//! let mut mem = GuestMemory::new();
//!
//! // 2. Add 4 KiB of memory at GPA 0x1000.
//! mem.add_region(Gpa::new(0x1000), 4096).unwrap();
//!
//! // 3. Write data to the guest memory.
//! mem.write_slice(Gpa::new(0x1000), b"Hello Guest!").unwrap();
//!
//! // 4. Read it back out.
//! let mut buf = [0u8; 12];
//! mem.read_slice(Gpa::new(0x1000), &mut buf).unwrap();
//! assert_eq!(&buf, b"Hello Guest!");
//! ```
//!
//! # The Fine Print
//! * **Page Alignment**: Memory blocks must be added in multiples of 4 KiB.
//! * **Windows Specific**: Behind the scenes, we use `VirtualAlloc` directly to ensure
//!   memory is physically committed and un-pageable if required by the hypervisor.

use std::ptr;

use hitz_hal::{Gpa, GuestMemAccess, HalError, MemFlags, Partition};
use tracing::debug;

use crate::error::MemError;

/// Page size used for alignment (4 KiB).
const PAGE_SIZE: usize = 4096;

/// A contiguous region of host memory mapped at a guest physical address.
///
/// Allocated via `VirtualAlloc` with `MEM_COMMIT | MEM_RESERVE` and freed
/// on drop via `VirtualFree`.
struct GuestRegion {
    /// Starting guest physical address for this region.
    gpa_start: Gpa,
    /// Size of the region in bytes (always page-aligned).
    size: usize,
    /// Host virtual address returned by `VirtualAlloc`.
    hva: *mut u8,
}

// SAFETY: The backing memory is exclusively owned by this struct.
// No other thread holds a pointer to it unless we explicitly hand one out,
// and all access goes through &self methods that use raw pointer ops which
// are inherently thread-unsafe at the CPU level but acceptable here because
// the VMM serializes access (single writer during setup, then the vCPU
// owns the mapping).
unsafe impl Send for GuestRegion {}
// SAFETY: Same reasoning as Send — exclusive ownership.
unsafe impl Sync for GuestRegion {}

impl Drop for GuestRegion {
    fn drop(&mut self) {
        // SAFETY: `self.hva` was allocated by `VirtualAlloc` with `MEM_COMMIT | MEM_RESERVE`.
        // Passing 0 for size with `MEM_RELEASE` frees the entire allocation.
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Memory::{MEM_RELEASE, VirtualFree};
            let _ = VirtualFree(self.hva.cast(), 0, MEM_RELEASE);
        }
    }
}

/// Guest physical memory manager.
///
/// Maintains a sorted list of internal regions and provides typed
/// read/write access by guest physical address. Regions can be mapped
/// into a HAL [`Partition`] for the hypervisor to wire up.
///
/// # Examples
/// ```
/// # use hitz_vmm::memory::GuestMemory;
/// # use hitz_hal::Gpa;
/// let mut mem = GuestMemory::new();
/// mem.add_region(Gpa::new(0x1000), 4096).unwrap();
/// ```
pub struct GuestMemory {
    /// Regions sorted by `gpa_start` (ascending).
    regions: Vec<GuestRegion>,
}

// SAFETY: All inner GuestRegions are Send + Sync.
unsafe impl Send for GuestMemory {}
// SAFETY: All inner GuestRegions are Send + Sync.
unsafe impl Sync for GuestMemory {}

impl GuestMemory {
    /// Create an empty guest memory with no regions.
    ///
    /// # Examples
    /// ```
    /// # use hitz_vmm::memory::GuestMemory;
    /// let mem = GuestMemory::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    /// Allocate a page-aligned host memory region and register it at `gpa`.
    ///
    /// `size` is rounded up to the next page boundary. The region is
    /// zero-initialized by the OS.
    pub fn add_region(&mut self, gpa: Gpa, size: usize) -> Result<(), MemError> {
        let aligned_size = align_up(size, PAGE_SIZE).ok_or(MemError::InvalidSize {
            gpa: gpa.as_u64(),
            size,
        })?;

        // Check for overlap before allocating.
        let gpa_end =
            gpa.as_u64()
                .checked_add(aligned_size as u64)
                .ok_or(MemError::InvalidSize {
                    gpa: gpa.as_u64(),
                    size: aligned_size,
                })?;
        for existing in &self.regions {
            let existing_end = existing
                .gpa_start
                .as_u64()
                .saturating_add(existing.size as u64);
            if gpa.as_u64() < existing_end && gpa_end > existing.gpa_start.as_u64() {
                return Err(MemError::Hal(hitz_hal::HalError::MapMemory {
                    gpa: gpa.as_u64(),
                    size: aligned_size,
                    reason: "region overlaps with an existing memory region".to_string(),
                }));
            }
        }

        let hva = virtual_alloc(aligned_size).ok_or(MemError::AllocFailed {
            gpa: gpa.as_u64(),
            size: aligned_size,
        })?;

        debug!(
            gpa = %gpa,
            size = aligned_size,
            hva = ?hva,
            "allocated guest memory region"
        );

        let region = GuestRegion {
            gpa_start: gpa,
            size: aligned_size,
            hva,
        };

        // Insert sorted by gpa_start.
        let pos = self
            .regions
            .binary_search_by_key(&gpa, |r| r.gpa_start)
            .unwrap_or_else(|i| i);
        self.regions.insert(pos, region);

        Ok(())
    }

    /// Map every region into a hypervisor partition.
    ///
    /// Calls [`Partition::map_memory`] for each region with the given flags.
    pub fn map_to_partition(
        &self,
        partition: &mut impl Partition,
        flags: MemFlags,
    ) -> Result<(), MemError> {
        for region in &self.regions {
            // SAFETY: `region.hva` points to a valid VirtualAlloc allocation of
            // `region.size` bytes that will remain valid for the lifetime of
            // this GuestMemory (which must outlive the partition mapping).
            unsafe {
                partition.map_memory(region.gpa_start, region.hva, region.size, flags)?;
            }
        }
        Ok(())
    }

    /// Resolve a GPA + length to a host pointer and the number of bytes
    /// remaining in that region from the resolved offset.
    ///
    /// Returns `Err(NotMapped)` if no region contains `gpa`, or
    /// `Err(OutOfBounds)` if `gpa + len` exceeds the region boundary.
    fn find_region(&self, gpa: Gpa, len: usize) -> Result<(*mut u8, usize), MemError> {
        let addr = gpa.as_u64();

        // Binary search for the region whose start is <= gpa.
        let idx = self
            .regions
            .binary_search_by(|r| {
                let start = r.gpa_start.as_u64();
                let end = start.saturating_add(r.size as u64);
                if addr < start {
                    std::cmp::Ordering::Greater
                } else if addr >= end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .map_err(|_| MemError::NotMapped { gpa: addr, len })?;

        let region = &self.regions[idx];
        // This VMM targets x86-64 only; 32-bit truncation is not a concern.
        #[allow(clippy::cast_possible_truncation)]
        let offset = (addr - region.gpa_start.as_u64()) as usize;
        let remaining = region.size - offset;

        if len > remaining {
            return Err(MemError::OutOfBounds { gpa: addr, len });
        }

        // SAFETY: offset is within bounds (checked above).
        let hva = unsafe { region.hva.add(offset) };
        Ok((hva, remaining))
    }

    /// Translate a guest physical address to a host virtual address.
    ///
    /// Returns `None` if the GPA is not mapped in any region.
    #[must_use]
    pub fn gpa_to_hva(&self, gpa: Gpa) -> Option<*mut u8> {
        self.find_region(gpa, 1).ok().map(|(hva, _)| hva)
    }

    /// Copy `data` into guest memory starting at `gpa`.
    ///
    /// # Examples
    /// ```
    /// # use hitz_vmm::memory::GuestMemory;
    /// # use hitz_hal::Gpa;
    /// let mut mem = GuestMemory::new();
    /// mem.add_region(Gpa::new(0), 4096).unwrap();
    /// mem.write_slice(Gpa::new(0), b"hitz rules").unwrap();
    /// ```
    pub fn write_slice(&self, gpa: Gpa, data: &[u8]) -> Result<(), MemError> {
        if data.is_empty() {
            return Ok(());
        }
        let (hva, _) = self.find_region(gpa, data.len())?;
        // SAFETY: find_region guarantees hva..hva+data.len() is within a valid
        // VirtualAlloc allocation. The source slice is valid by construction.
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), hva, data.len());
        }
        Ok(())
    }

    /// Read `buf.len()` bytes from guest memory at `gpa` into `buf`.
    ///
    /// # Examples
    /// ```
    /// # use hitz_vmm::memory::GuestMemory;
    /// # use hitz_hal::Gpa;
    /// let mut mem = GuestMemory::new();
    /// mem.add_region(Gpa::new(0), 4096).unwrap();
    /// mem.write_slice(Gpa::new(0), b"test").unwrap();
    /// let mut out = [0u8; 4];
    /// mem.read_slice(Gpa::new(0), &mut out).unwrap();
    /// assert_eq!(&out, b"test");
    /// ```
    pub fn read_slice(&self, gpa: Gpa, buf: &mut [u8]) -> Result<(), MemError> {
        if buf.is_empty() {
            return Ok(());
        }
        let (hva, _) = self.find_region(gpa, buf.len())?;
        // SAFETY: find_region guarantees hva..hva+buf.len() is within a valid
        // VirtualAlloc allocation. The destination slice is valid by construction.
        unsafe {
            ptr::copy_nonoverlapping(hva, buf.as_mut_ptr(), buf.len());
        }
        Ok(())
    }

    /// Write a `Copy` value into guest memory at `gpa`.
    /// Write a `u64` value into guest memory at `gpa`.
    pub fn write_u64(&self, gpa: Gpa, val: u64) -> Result<(), MemError> {
        self.write_slice(gpa, &val.to_ne_bytes())
    }

    /// Write a `u32` value into guest memory at `gpa`.
    pub fn write_u32(&self, gpa: Gpa, val: u32) -> Result<(), MemError> {
        self.write_slice(gpa, &val.to_ne_bytes())
    }

    /// Read a `u64` value from guest memory at `gpa`.
    pub fn read_u64(&self, gpa: Gpa) -> Result<u64, MemError> {
        let mut buf = [0u8; 8];
        self.read_slice(gpa, &mut buf)?;
        Ok(u64::from_ne_bytes(buf))
    }

    /// Read a `u32` value from guest memory at `gpa`.
    pub fn read_u32(&self, gpa: Gpa) -> Result<u32, MemError> {
        let mut buf = [0u8; 4];
        self.read_slice(gpa, &mut buf)?;
        Ok(u32::from_ne_bytes(buf))
    }
}

impl Default for GuestMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl GuestMemAccess for GuestMemory {
    fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), HalError> {
        self.read_slice(Gpa::new(gpa), buf)
            .map_err(|e| HalError::GuestMem {
                gpa,
                reason: e.to_string(),
            })
    }

    fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), HalError> {
        self.write_slice(Gpa::new(gpa), data)
            .map_err(|e| HalError::GuestMem {
                gpa,
                reason: e.to_string(),
            })
    }
}

/// Implement the boot crate's `GuestMemWriter` trait so `load_elf` and
/// friends can write directly into guest RAM.
impl hitz_boot::GuestMemWriter for GuestMemory {
    fn write_bytes(&self, gpa: Gpa, data: &[u8]) -> Result<(), hitz_boot::BootError> {
        self.write_slice(gpa, data)
            .map_err(|e| hitz_boot::BootError::WriteFailed(e.to_string()))
    }

    fn write_zeroes(&self, mut gpa: Gpa, mut len: usize) -> Result<(), hitz_boot::BootError> {
        // Removed unnecessary heap allocation when zeroing memory.
        // We use a fixed-size stack buffer and loop to avoid `unsafe` and handle arbitrarily large regions.
        const CHUNK_SIZE: usize = 4096;
        let zeroes = [0u8; CHUNK_SIZE];
        while len > 0 {
            let chunk = len.min(CHUNK_SIZE);
            self.write_slice(gpa, &zeroes[..chunk])
                .map_err(|e| hitz_boot::BootError::WriteFailed(e.to_string()))?;
            gpa = Gpa::new(gpa.as_u64().saturating_add(chunk as u64));
            len -= chunk;
        }
        Ok(())
    }
}

/// Round `value` up to the next multiple of `align`.
///
/// `align` must be a power of two.
const fn align_up(value: usize, align: usize) -> Option<usize> {
    match value.checked_add(align - 1) {
        Some(v) => Some(v & !(align - 1)),
        None => None,
    }
}

/// Allocate `size` bytes of committed, read-write, page-aligned memory.
///
/// Returns `None` if the allocation fails.
#[cfg(windows)]
fn virtual_alloc(size: usize) -> Option<*mut u8> {
    use windows::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};

    // SAFETY: Passing null for lpAddress lets the OS choose the base.
    // MEM_COMMIT | MEM_RESERVE allocates and commits in one call.
    let ptr = unsafe { VirtualAlloc(None, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };

    if ptr.is_null() {
        None
    } else {
        Some(ptr.cast::<u8>())
    }
}

/// Fallback for non-Windows platforms (tests on CI, etc.).
///
/// Uses a standard `Vec` allocation as a stand-in. The pointer is leaked
/// intentionally — `GuestRegion::drop` is a no-op on non-Windows, so the
/// memory is reclaimed when the process exits.
#[cfg(not(windows))]
fn virtual_alloc(size: usize) -> Option<*mut u8> {
    let mut buf = vec![0u8; size];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    Some(ptr)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn test_add_region() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0x1000), 4096)
            .expect("add_region should succeed");
        assert_eq!(mem.regions.len(), 1);
        assert_eq!(mem.regions[0].gpa_start, Gpa::new(0x1000));
        assert_eq!(mem.regions[0].size, 4096);
        assert!(!mem.regions[0].hva.is_null());
    }

    #[test]
    fn test_add_overlapping_regions_fails() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0x1000), 4096)
            .expect("add_region should succeed");
        let err = mem
            .add_region(Gpa::new(0x1500), 4096)
            .expect_err("should return error on overlap");
        assert!(matches!(
            err,
            MemError::Hal(hitz_hal::HalError::MapMemory { .. })
        ));
        let err2 = mem
            .add_region(Gpa::new(0x800), 4096)
            .expect_err("should return error on overlap");
        assert!(matches!(
            err2,
            MemError::Hal(hitz_hal::HalError::MapMemory { .. })
        ));
    }

    #[test]
    fn test_write_read_roundtrip() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0), 4096)
            .expect("add_region should succeed");

        let data = b"hello hitz";
        mem.write_slice(Gpa::new(0), data)
            .expect("write should succeed");

        let mut buf = vec![0u8; data.len()];
        mem.read_slice(Gpa::new(0), &mut buf)
            .expect("read should succeed");

        assert_eq!(&buf, data);
    }

    #[test]
    fn test_write_u64_read_u64() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0), 4096)
            .expect("add_region should succeed");

        let val: u64 = 0xDEAD_BEEF_CAFE_BABE;
        mem.write_u64(Gpa::new(64), val)
            .expect("write_u64 should succeed");

        let read_back = mem.read_u64(Gpa::new(64)).expect("read_u64 should succeed");

        assert_eq!(read_back, val);
    }

    #[test]
    fn test_gpa_to_hva() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0x10_0000), 0x10_0000)
            .expect("add_region should succeed");

        // Start of region.
        let hva_start = mem.gpa_to_hva(Gpa::new(0x10_0000));
        assert!(hva_start.is_some());

        // Middle of region.
        let hva_mid = mem.gpa_to_hva(Gpa::new(0x18_0000));
        assert!(hva_mid.is_some());

        // Just inside the end of the region.
        let hva_end = mem.gpa_to_hva(Gpa::new(0x1F_FFFF));
        assert!(hva_end.is_some());

        // The HVA offsets should match the GPA offsets.
        let base = hva_start.expect("base should be Some");
        let mid = hva_mid.expect("mid should be Some");
        assert_eq!(
            (mid as usize) - (base as usize),
            0x8_0000,
            "HVA offset should match GPA offset"
        );

        // Outside the region.
        assert!(mem.gpa_to_hva(Gpa::new(0x0F_FFFF)).is_none());
        assert!(mem.gpa_to_hva(Gpa::new(0x20_0000)).is_none());
    }

    #[test]
    fn test_out_of_bounds() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0), 4096)
            .expect("add_region should succeed");

        // Writing past end of region should fail.
        let big = vec![0xABu8; 8192];
        let err = mem.write_slice(Gpa::new(0), &big);
        assert!(err.is_err());
        assert!(
            matches!(err, Err(MemError::OutOfBounds { .. })),
            "expected OutOfBounds, got {err:?}"
        );

        // Writing at an offset that overflows should also fail.
        let small = [0u8; 16];
        let err = mem.write_slice(Gpa::new(4090), &small);
        assert!(err.is_err());
        assert!(
            matches!(err, Err(MemError::OutOfBounds { .. })),
            "expected OutOfBounds, got {err:?}"
        );
    }

    #[test]
    fn test_not_mapped() {
        let mem = GuestMemory::new();

        // No regions at all — any access should fail.
        let mut buf = [0u8; 4];
        let err = mem.read_slice(Gpa::new(0x1000), &mut buf);
        assert!(err.is_err());
        assert!(
            matches!(err, Err(MemError::NotMapped { .. })),
            "expected NotMapped, got {err:?}"
        );
    }

    #[test]
    fn test_multiple_regions() {
        let mut mem = GuestMemory::new();
        // Two non-overlapping regions.
        mem.add_region(Gpa::new(0x0000), 4096)
            .expect("add_region 1 should succeed");
        mem.add_region(Gpa::new(0x10_0000), 4096)
            .expect("add_region 2 should succeed");

        assert_eq!(mem.regions.len(), 2);

        // Write different patterns to each.
        mem.write_slice(Gpa::new(0), b"region-one")
            .expect("write to region 1");
        mem.write_slice(Gpa::new(0x10_0000), b"region-two")
            .expect("write to region 2");

        // Read them back independently.
        let mut buf1 = vec![0u8; 10];
        let mut buf2 = vec![0u8; 10];
        mem.read_slice(Gpa::new(0), &mut buf1)
            .expect("read from region 1");
        mem.read_slice(Gpa::new(0x10_0000), &mut buf2)
            .expect("read from region 2");

        assert_eq!(&buf1, b"region-one");
        assert_eq!(&buf2, b"region-two");

        // Gap between regions should not be mapped.
        let mut gap_buf = [0u8; 1];
        let err = mem.read_slice(Gpa::new(0x8_0000), &mut gap_buf);
        assert!(
            matches!(err, Err(MemError::NotMapped { .. })),
            "gap should not be mapped"
        );
    }

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 4096), Some(0));
        assert_eq!(align_up(1, 4096), Some(4096));
        assert_eq!(align_up(4096, 4096), Some(4096));
        assert_eq!(align_up(4097, 4096), Some(8192));
        assert_eq!(align_up(8191, 4096), Some(8192));
    }

    #[test]
    fn test_regions_sorted() {
        let mut mem = GuestMemory::new();
        // Insert out of order.
        mem.add_region(Gpa::new(0x30_0000), 4096)
            .expect("add_region should succeed");
        mem.add_region(Gpa::new(0x10_0000), 4096)
            .expect("add_region should succeed");
        mem.add_region(Gpa::new(0x20_0000), 4096)
            .expect("add_region should succeed");

        // Verify sorted order.
        assert_eq!(mem.regions[0].gpa_start, Gpa::new(0x10_0000));
        assert_eq!(mem.regions[1].gpa_start, Gpa::new(0x20_0000));
        assert_eq!(mem.regions[2].gpa_start, Gpa::new(0x30_0000));
    }

    #[test]
    fn test_empty_operations() {
        let mut mem = GuestMemory::new();
        mem.add_region(Gpa::new(0), 4096)
            .expect("add_region should succeed");

        // Empty write and read should be no-ops.
        mem.write_slice(Gpa::new(0), &[]).expect("empty write");
        mem.read_slice(Gpa::new(0), &mut []).expect("empty read");
    }

    #[test]
    fn test_align_up_overflow() {
        assert_eq!(align_up(usize::MAX, PAGE_SIZE), None);
    }

    #[test]
    fn test_add_region_overflow() {
        let mut mem = GuestMemory::new();
        let result = mem.add_region(Gpa::new(0), usize::MAX);
        assert!(matches!(result, Err(MemError::InvalidSize { .. })));
    }
}
