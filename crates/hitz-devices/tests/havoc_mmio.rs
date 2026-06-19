#![allow(missing_docs)]

use hitz_devices::{MmioBus, MmioDevice};
use hitz_hal::{GuestMemAccess, HalError};

struct StubMem;
impl GuestMemAccess for StubMem {
    fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), HalError> { Ok(()) }
    fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), HalError> { Ok(()) }
}

struct StubDevice;
impl MmioDevice for StubDevice {
    fn mmio_read(&mut self, _offset: u64, _data: &mut [u8]) {}
    fn mmio_write(&mut self, _offset: u64, _data: &[u8], _mem: &dyn GuestMemAccess) -> Option<u8> { None }
    fn poll_rx(&mut self) -> Option<u8> { None }
}

#[test]
#[should_panic(expected = "MMIO region overflows address space")]
fn havoc_mmio_overlap() {
    let mut bus = MmioBus::new();

    // Now even an empty bus should catch this
    bus.register(u64::MAX - 50, 1000, Box::new(StubDevice));
}
