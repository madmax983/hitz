use super::VirtQueue;
use hitz_hal::GuestMemAccess;
use hitz_hal::HalError;

struct DummyMem;
impl GuestMemAccess for DummyMem {
    fn read_guest(&self, _gpa: u64, buf: &mut [u8]) -> Result<(), HalError> {
        buf.fill(0);
        Ok(())
    }
    fn write_guest(&self, _gpa: u64, _buf: &[u8]) -> Result<(), HalError> {
        Ok(())
    }
}

#[test]
fn exploit_push_used_overflow() {
    let mut q = VirtQueue::new(256);
    q.configure(0, 0, u64::MAX - 2);
    q.set_ready(true);
    let mem = DummyMem;
    q.push_used(&mem, 0, 0); // No panic now!
}
