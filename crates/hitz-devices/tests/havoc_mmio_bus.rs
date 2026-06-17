#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
use hitz_devices::{MmioBus, MmioDevice};

struct DummyDevice;
impl MmioDevice for DummyDevice {
    fn mmio_read(&mut self, _offset: u64, _data: &mut [u8]) {}
    fn mmio_write(
        &mut self,
        _offset: u64,
        _data: &[u8],
        _mem: &dyn hitz_hal::GuestMemAccess,
    ) -> Option<u8> {
        None
    }
}

#[test]
fn havoc_test_register_overflow() {
    let mut bus = MmioBus::new();
    bus.register(0, 10, Box::new(DummyDevice));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bus.register(u64::MAX - 5, 20, Box::new(DummyDevice));
    }));

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Check if the panic message matches
    let msg = err.downcast_ref::<String>().map_or_else(
        || err.downcast_ref::<&str>().unwrap_or(&"unknown").to_string(),
        std::clone::Clone::clone,
    );

    assert!(
        msg.contains("MMIO region size overflow"),
        "Expected MMIO region size overflow panic"
    );
}
