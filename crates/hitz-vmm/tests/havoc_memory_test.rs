#![allow(clippy::unwrap_used)]

use hitz_hal::Gpa;
use hitz_vmm::{GuestMemory, MemError};

/// Tests that adding a memory region that overflows `u64::MAX` safely
/// returns an `InvalidSize` error instead of panicking.
#[test]
fn havoc_test_add_region_integer_overflow() {
    let mut mem = GuestMemory::new();

    // Attempt to add a region whose `gpa_start + size` exceeds u64::MAX
    // Using a large GPA and a large aligned size
    let result = mem.add_region(Gpa::new(u64::MAX - 0x100), 0x2000);

    assert!(
        matches!(result, Err(MemError::InvalidSize { .. })),
        "adding a region that overflows u64 should return an InvalidSize error, got: {result:?}"
    );
}
