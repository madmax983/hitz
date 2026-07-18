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
fn should_return_none_when_avail_idx_read_overflows() {
    let mut q = VirtQueue::new(16);
    q.configure(0, u64::MAX - 1, 0);
    q.set_ready(true);
    let mem = DummyMem;
    assert!(q.pop_chain(&mem).is_none());
}

#[test]
fn should_return_none_when_head_idx_read_overflows() {
    let mut q = VirtQueue::new(16);
    q.configure(0, u64::MAX - 3, 0);
    q.set_ready(true);
    let mem = DummyMem;
    assert!(q.pop_chain(&mem).is_none());
}

#[test]
fn should_return_none_when_next_descriptor_read_overflows() {
    let mut q = VirtQueue::new(16);
    q.configure(u64::MAX - 2, 0, 0);
    q.set_ready(true);
    let mem = DummyMem;

    // We instantiate a chain where reading the descriptor at index 1 will overflow the GPA.
    let mut chain = super::DescriptorChain {
        desc_gpa: u64::MAX - 2,
        queue_size: 16,
        head_idx: 0,
        next_idx: Some(1),
        count: 0,
    };
    assert!(chain.next_descriptor(&mem).is_none());
}

#[test]
fn should_return_early_when_used_ring_offset_overflows() {
    let mem = DummyMem;
    let mut q = VirtQueue::new(256);
    // Setting used_gpa such that calculating elem_gpa overflows:
    // used_idx * 8 + 4 + used_gpa > u64::MAX
    q.configure(0, 0, u64::MAX - 6);
    q.set_ready(true);
    q.push_used(&mem, 0, 0); // Should return early without panic
}

#[test]
fn should_return_early_when_used_elem_len_gpa_overflows() {
    let mem = DummyMem;
    let mut q = VirtQueue::new(256);
    // Setting used_gpa such that calculating elem_len_gpa overflows:
    // used_idx * 8 + 8 + used_gpa > u64::MAX
    q.configure(0, 0, u64::MAX - 10);
    q.set_ready(true);
    q.push_used(&mem, 0, 0); // Should return early without panic
}
