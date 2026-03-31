#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
use hitz_hal::Gpa;
use hitz_vmm::GuestMemory;
use proptest::prelude::*;

proptest! {
    #[test]
    fn torture_guest_memory_write_read(
        gpa1 in 0..10_000u64,
        size1 in 1..4096usize,
        write_addr in 0..10_000u64,
        write_data in prop::collection::vec(any::<u8>(), 0..5000)
    ) {
        let mut mem = GuestMemory::new();
        // Ignore errors if size is too big or invalid
        let _ = mem.add_region(Gpa::new(gpa1), size1);

        // This should either succeed or fail safely, not panic
        let _ = mem.write_slice(Gpa::new(write_addr), &write_data);

        let mut read_buf = vec![0u8; write_data.len()];
        let _ = mem.read_slice(Gpa::new(write_addr), &mut read_buf);
    }
}
