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

#[test]
fn test_push_used_elem_gpa_overflow() {
    let mut q = VirtQueue::new(256);
    // Setting up the queue so that elem_gpa is close to u64::MAX, but
    // elem_gpa.checked_add(4) in push_used fails
    // used_gpa + 4 + ring_offset = u64::MAX - 3
    // Since dummy mem returns 0 for read, used_idx is 0.
    // ring_offset = 0
    // We want used_gpa + 4 = u64::MAX - 3 -> used_gpa = u64::MAX - 7
    q.configure(0, 0, u64::MAX - 7);
    q.set_ready(true);
    let mem = DummyMem;

    q.push_used(&mem, 0, 512); // Should not panic
}

#[test]
fn test_pop_chain_avail_idx_gpa_overflow() {
    let mut q = VirtQueue::new(256);
    q.configure(0, u64::MAX - 1, 0);
    q.set_ready(true);
    let mem = DummyMem;

    let _ = q.pop_chain(&mem); // Should not panic
}

#[test]
fn test_pop_chain_head_idx_gpa_overflow() {
    let mut q = VirtQueue::new(256);
    q.configure(0, u64::MAX - 3, 0);
    q.set_ready(true);
    let mem = DummyMem;

    let _ = q.pop_chain(&mem); // Should not panic
}

#[test]
fn test_next_descriptor_desc_addr_overflow() {
    let mut q = VirtQueue::new(256);
    q.configure(u64::MAX - 10, 0, 0);
    q.set_ready(true);
    let mem = DummyMem;

    // Simulate returning a valid avail_idx and head_idx from dummy mem, but we don't have to
    // we can just construct DescriptorChain manually
    let mut chain = super::DescriptorChain {
        desc_gpa: u64::MAX - 10,
        queue_size: 256,
        head_idx: 0,
        next_idx: Some(1),
        count: 0,
    };

    let _ = chain.next_descriptor(&mem); // Should not panic
}
