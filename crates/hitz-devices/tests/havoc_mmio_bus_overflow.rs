#![allow(missing_docs)]

use hitz_devices::{MmioBus, MmioDevice};
use hitz_hal::GuestMemAccess;

struct StubDevice;
impl MmioDevice for StubDevice {
    fn mmio_read(&mut self, _offset: u64, _data: &mut [u8]) {}
    fn mmio_write(&mut self, _offset: u64, _data: &[u8], _mem: &dyn GuestMemAccess) -> Option<u8> { None }
}

#[test]
#[should_panic(expected = "Overlapping MMIO region")]
fn havoc_mmio_bus_register_overflow() {
    let mut bus = MmioBus::new();

    // In order for `base_gpa < slot_end && end_gpa > slot.base` to be true:
    // With our fixes:
    // let end_gpa = (u64::MAX - 5).checked_add(10).unwrap_or(u64::MAX) -> u64::MAX
    // If the existing slot is: base = 0, size = u64::MAX -> slot_end = u64::MAX

    bus.register(0, u64::MAX, Box::new(StubDevice));

    bus.register(u64::MAX - 5, 10, Box::new(StubDevice));
}
