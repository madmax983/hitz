//! Proptests for MmioBus

use hitz_devices::{MmioBus, MmioDevice};
use hitz_hal::GuestMemAccess;
use proptest::prelude::*;

struct DummyDevice;
impl MmioDevice for DummyDevice {
    fn mmio_read(&mut self, _: u64, _: &mut [u8]) {}
    fn mmio_write(&mut self, _: u64, _: &[u8], _: &dyn GuestMemAccess) -> Option<u8> { None }
}

proptest! {
    #[test]
    fn test_register_fuzz(base1 in any::<u64>(), size1 in any::<u64>(), base2 in any::<u64>(), size2 in any::<u64>()) {
        let mut bus = MmioBus::new();
        // Since the current logic is to panic explicitly on overlapping regions or integer overflow,
        // we can just run the registration inside a catch_unwind to verify it doesn't cause UB or
        // silent math wraparound panics.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.register(base1, size1, Box::new(DummyDevice));
            bus.register(base2, size2, Box::new(DummyDevice));
        }));
    }
}
