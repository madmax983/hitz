//! Custom virtqueue implementation reading all structures from guest memory.
//!
//! # Abstract
//! No dependency on `vm-memory` or `virtio-queue` crates. All descriptor,
//! available, and used ring accesses go through [`GuestMemAccess`].
//!
//! # The Hero's Journey
//! ```
//! # use hitz_devices::VirtQueue;
//! // 1. Create a queue of a power of two size.
//! let mut queue = VirtQueue::new(256);
//!
//! // 2. Guest sets up GPAs for descriptors, available, and used rings.
//! queue.configure(0x1000, 0x2000, 0x3000);
//!
//! // 3. Set the queue to ready!
//! queue.set_ready(true);
//! ```

use hitz_hal::GuestMemAccess;

/// Descriptor flag: this descriptor continues via `next`.
const VIRTQ_DESC_F_NEXT: u16 = 1;
/// Descriptor flag: buffer is device-writable (otherwise device-readable).
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Maximum supported queue depth.
const MAX_QUEUE_SIZE: u16 = 256;

/// A single virtqueue, managing descriptor, available, and used rings.
///
/// # Abstract
///
/// This structure represents a single Virtio queue (Split Virtqueue). It maintains
/// pointers to the three vital areas in guest memory: the Descriptor Table, the
/// Available Ring, and the Used Ring.
///
/// # The Hero's Journey
///
/// Typically, `VirtQueue` instances are created automatically by the `VirtioMmioTransport`
/// layer as the guest OS initializes them. Your backend device interacts with them
/// when processing a queue:
///
/// ```
/// # use hitz_devices::VirtQueue;
/// # use hitz_hal::GuestMemAccess;
/// # fn handle_queue(queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
/// // 1. Pop the next available descriptor chain the guest has produced.
/// while let Some(mut chain) = queue.pop_chain(mem) {
///     // 2. Process descriptors in the chain...
///     let head_idx = chain.head_index();
///
///     // 3. Mark the chain as used, returning it to the guest.
///     queue.push_used(mem, head_idx, 0);
/// }
/// # }
/// ```
///
/// # The Fine Print
///
/// The queue size is bounded (typically 256 for Hitz) and must be a power of two.
/// The VMM interacts with the raw memory structures through a [`GuestMemAccess`] trait
/// object to avoid mapping guest rings directly into host virtual memory permanently.
pub struct VirtQueue {
    /// Queue depth (power of 2, max 256).
    size: u16,
    /// Guest physical address of the descriptor table.
    desc_gpa: u64,
    /// Guest physical address of the available ring.
    avail_gpa: u64,
    /// Guest physical address of the used ring.
    used_gpa: u64,
    /// Index of the next available ring entry to consume.
    last_avail_idx: u16,
    /// Whether the queue is ready for use.
    ready: bool,
}

impl VirtQueue {
    /// Create a new virtqueue with the given depth.
    ///
    /// `size` must be a power of 2 and at most `MAX_QUEUE_SIZE` (256).
    /// If `size` is 0 or exceeds the maximum, it is clamped to `MAX_QUEUE_SIZE`.
    ///
    /// # Examples
    /// ```
    /// # use hitz_devices::VirtQueue;
    /// let queue = VirtQueue::new(128);
    /// ```
    #[must_use]
    pub const fn new(size: u16) -> Self {
        let size = if size == 0 || size > MAX_QUEUE_SIZE {
            MAX_QUEUE_SIZE
        } else {
            size
        };
        Self {
            size,
            desc_gpa: 0,
            avail_gpa: 0,
            used_gpa: 0,
            last_avail_idx: 0,
            ready: false,
        }
    }

    /// Set the guest physical addresses for the three ring structures.
    pub const fn configure(&mut self, desc_gpa: u64, avail_gpa: u64, used_gpa: u64) {
        self.desc_gpa = desc_gpa;
        self.avail_gpa = avail_gpa;
        self.used_gpa = used_gpa;
    }

    /// Mark the queue as ready or not ready.
    pub const fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }

    /// Returns `true` if the queue has been marked ready.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    /// Exposes the current capacity of the virtqueue.
    ///
    /// # Abstract
    /// This is the maximum number of descriptors that this queue can hold in
    /// its ring buffer simultaneously. The guest OS queries this during the
    /// virtio discovery phase to allocate appropriately sized structures in memory.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use hitz_devices::virtio::queue::VirtQueue;
    ///
    /// let q = VirtQueue::new(128);
    /// assert_eq!(q.size(), 128);
    /// ```
    #[must_use]
    pub const fn size(&self) -> u16 {
        self.size
    }

    /// Set the queue size (called when guest writes `QueueNum`).
    pub const fn set_size(&mut self, size: u16) {
        if size > 0 && size <= MAX_QUEUE_SIZE {
            self.size = size;
        }
    }

    /// Reset the queue to its initial state.
    pub const fn reset(&mut self) {
        self.desc_gpa = 0;
        self.avail_gpa = 0;
        self.used_gpa = 0;
        self.last_avail_idx = 0;
        self.ready = false;
        self.size = MAX_QUEUE_SIZE;
    }

    /// Pop the next available descriptor chain from the queue.
    ///
    /// Returns `None` if the queue is empty or not ready.
    /// On success, returns a [`DescriptorChain`] that iterates over
    /// the descriptors in the chain, plus the head index for later
    /// use in [`push_used`](Self::push_used).
    #[allow(rustdoc::private_intra_doc_links)]
    pub fn pop_chain(&mut self, mem: &dyn GuestMemAccess) -> Option<DescriptorChain> {
        if !self.ready {
            return None;
        }

        // Read avail.idx (u16 at offset 2 in the available ring).
        let avail_idx_gpa = self.avail_gpa.checked_add(2)?;
        let avail_idx = read_u16(mem, avail_idx_gpa)?;

        // Nothing new?
        if self.last_avail_idx == avail_idx {
            return None;
        }

        // Read the head descriptor index from the ring.
        // avail ring layout: flags(u16), idx(u16), ring[size](u16 each)
        let ring_offset = u64::from(self.last_avail_idx % self.size) * 2;
        let head_idx_gpa = self.avail_gpa.checked_add(4)?.checked_add(ring_offset)?;
        let head_idx = read_u16(mem, head_idx_gpa)?;

        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);

        Some(DescriptorChain {
            desc_gpa: self.desc_gpa,
            queue_size: self.size,
            head_idx,
            next_idx: Some(head_idx),
            count: 0,
        })
    }

    /// Write an entry to the used ring and bump used.idx.
    ///
    /// `head_idx` is the descriptor chain head (from [`DescriptorChain::head_index`]).
    /// `len` is the total number of bytes written to device-writable descriptors.
    #[allow(rustdoc::private_intra_doc_links)]
    pub fn push_used(&self, mem: &dyn GuestMemAccess, head_idx: u16, len: u32) {
        // Read current used.idx.
        let Some(idx_gpa) = self.used_gpa.checked_add(2) else {
            return;
        };
        let Some(used_idx) = read_u16(mem, idx_gpa) else {
            return;
        };

        // Used ring layout: flags(u16), idx(u16), ring[size](VirtqUsedElem: id(u32) + len(u32))
        let ring_offset = u64::from(used_idx % self.size) * 8;
        let Some(elem_gpa) = self
            .used_gpa
            .checked_add(4)
            .and_then(|x| x.checked_add(ring_offset))
        else {
            return;
        };

        // Write VirtqUsedElem { id, len }.
        let _ = mem.write_guest(elem_gpa, &u32::from(head_idx).to_le_bytes());
        if let Some(elem_len_gpa) = elem_gpa.checked_add(4) {
            let _ = mem.write_guest(elem_len_gpa, &len.to_le_bytes());
        }

        // Bump used.idx.
        let new_idx = used_idx.wrapping_add(1);
        let _ = mem.write_guest(idx_gpa, &new_idx.to_le_bytes());
    }
}

/// An iterator over descriptors in a chain.
///
/// # Abstract
/// Represents a chain of descriptors starting from the head descriptor.
/// It yields `Descriptor` elements containing the address, length, and access rights.
///
/// # The Hero's Journey
/// ```
/// # use hitz_devices::VirtQueue;
/// // Handled internally by `VirtQueue`.
/// ```
pub struct DescriptorChain {
    /// Base GPA of the descriptor table.
    desc_gpa: u64,
    /// Queue size for bounds checking.
    queue_size: u16,
    /// Head descriptor index (for `push_used`).
    head_idx: u16,
    /// Next descriptor index to read, or `None` if chain is exhausted.
    next_idx: Option<u16>,
    /// Number of descriptors processed so far (prevents infinite loops on cycles).
    count: u16,
}

/// A single descriptor from a chain.
///
/// # Abstract
/// Defines a single buffer's guest physical address, its size in bytes,
/// and whether it's writable by the device.
///
/// # The Hero's Journey
/// ```
/// use hitz_devices::virtio::queue::Descriptor;
///
/// // Descriptors are typically returned by the chain via the VirtQueue.
/// // They are simple structs containing GPA and length.
/// // Example of what a descriptor would look like when populated:
/// let desc = Descriptor { gpa: 0x1000, len: 4096, is_device_writable: true };
/// assert_eq!(desc.len, 4096);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// Guest physical address of the buffer.
    pub gpa: u64,
    /// Length of the buffer in bytes.
    pub len: u32,
    /// `true` if the buffer is device-writable (guest expects data back).
    pub is_device_writable: bool,
}

impl DescriptorChain {
    /// Retrieves the index of the first descriptor in this chain.
    ///
    /// # Abstract
    /// When the device finishes processing a request chain, it must signal
    /// completion to the guest by pushing exactly this head index into the
    /// Used Ring.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// // Example of querying the head index from a valid chain.
    /// # use hitz_devices::virtio::queue::VirtQueue;
    /// # use hitz_hal::{GuestMemAccess, HalError};
    /// # struct DummyMem;
    /// # impl GuestMemAccess for DummyMem {
    /// #     fn read_guest(&self, _gpa: u64, buf: &mut [u8]) -> Result<(), HalError> { buf.fill(0); Ok(()) }
    /// #     fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), HalError> { Ok(()) }
    /// # }
    /// # let mem = DummyMem;
    /// # let mut q = VirtQueue::new(16);
    /// # // Manually simulate `last_avail_idx != avail_idx` to force `pop_chain` to return a mocked chain
    /// # q.set_ready(true);
    /// # // For the sake of the doctest, we just show the API. Real `pop_chain` requires valid guest memory.
    /// // let chain = queue.pop_chain(&mem).unwrap();
    /// // assert_eq!(chain.head_index(), 42);
    /// ```
    #[must_use]
    pub const fn head_index(&self) -> u16 {
        self.head_idx
    }

    /// Read the next descriptor in the chain from guest memory.
    ///
    /// Returns `None` when the chain is exhausted or on read error.
    pub fn next_descriptor(&mut self, mem: &dyn GuestMemAccess) -> Option<Descriptor> {
        let idx = self.next_idx?;

        // Bounds check.
        if idx >= self.queue_size {
            self.next_idx = None;
            return None;
        }

        // Prevent infinite loops from cyclic descriptor chains.
        if self.count >= self.queue_size {
            tracing::warn!(
                head_idx = self.head_idx,
                "virtio descriptor chain exceeded queue size (cycle detected), aborting"
            );
            self.next_idx = None;
            return None;
        }
        self.count += 1;

        // Each VirtqDesc is 16 bytes: addr(u64) + len(u32) + flags(u16) + next(u16).
        let desc_addr = self.desc_gpa.checked_add(u64::from(idx) * 16)?;
        let mut buf = [0u8; 16];
        if mem.read_guest(desc_addr, &mut buf).is_err() {
            self.next_idx = None;
            return None;
        }

        let gpa = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let len = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let flags = u16::from_le_bytes([buf[12], buf[13]]);
        let next = u16::from_le_bytes([buf[14], buf[15]]);

        let is_device_writable = flags & VIRTQ_DESC_F_WRITE != 0;

        // Follow chain if NEXT flag is set.
        if flags & VIRTQ_DESC_F_NEXT != 0 {
            self.next_idx = Some(next);
        } else {
            self.next_idx = None;
        }

        Some(Descriptor {
            gpa,
            len,
            is_device_writable,
        })
    }
}

/// Read a little-endian u16 from guest memory, returning `None` on error.
fn read_u16(mem: &dyn GuestMemAccess, gpa: u64) -> Option<u16> {
    let mut buf = [0u8; 2];
    mem.read_guest(gpa, &mut buf).ok()?;
    Some(u16::from_le_bytes(buf))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock guest memory backed by a `Vec<u8>`.
    struct MockMem {
        inner: Mutex<Vec<u8>>,
    }

    impl MockMem {
        fn new(size: usize) -> Self {
            Self {
                inner: Mutex::new(vec![0u8; size]),
            }
        }

        fn write_bytes(&self, offset: u64, data: &[u8]) {
            let mut mem = self.inner.lock().expect("device lock poisoned");
            #[allow(clippy::cast_possible_truncation)]
            let start = offset as usize;
            mem[start..start + data.len()].copy_from_slice(data);
        }
    }

    impl GuestMemAccess for MockMem {
        fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), hitz_hal::HalError> {
            let mem = self.inner.lock().expect("device lock poisoned");
            #[allow(clippy::cast_possible_truncation)]
            let start = gpa as usize;
            if start + buf.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: buf.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            buf.copy_from_slice(&mem[start..start + buf.len()]);
            drop(mem);
            Ok(())
        }

        fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), hitz_hal::HalError> {
            let mut mem = self.inner.lock().expect("device lock poisoned");
            #[allow(clippy::cast_possible_truncation)]
            let start = gpa as usize;
            if start + data.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: data.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            mem[start..start + data.len()].copy_from_slice(data);
            drop(mem);
            Ok(())
        }
    }

    // Memory layout constants for tests.
    const DESC_BASE: u64 = 0x0000; // Descriptor table
    const AVAIL_BASE: u64 = 0x1000; // Available ring
    const USED_BASE: u64 = 0x2000; // Used ring

    /// Write a descriptor to the mock memory at the given index.
    fn write_desc(mem: &MockMem, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = DESC_BASE + u64::from(idx) * 16;
        mem.write_bytes(offset, &addr.to_le_bytes());
        mem.write_bytes(offset + 8, &len.to_le_bytes());
        mem.write_bytes(offset + 12, &flags.to_le_bytes());
        mem.write_bytes(offset + 14, &next.to_le_bytes());
    }

    /// Write an entry to the available ring at position `ring_idx`.
    fn write_avail_entry(mem: &MockMem, ring_idx: u16, desc_idx: u16) {
        let offset = AVAIL_BASE + 4 + u64::from(ring_idx) * 2;
        mem.write_bytes(offset, &desc_idx.to_le_bytes());
    }

    /// Set avail.idx.
    fn set_avail_idx(mem: &MockMem, idx: u16) {
        mem.write_bytes(AVAIL_BASE + 2, &idx.to_le_bytes());
    }

    /// Read used.idx.
    fn read_used_idx(mem: &MockMem) -> u16 {
        let mut buf = [0u8; 2];
        mem.read_guest(USED_BASE + 2, &mut buf)
            .expect("device lock poisoned");
        u16::from_le_bytes(buf)
    }

    /// Read a used ring element (id, len) at position `ring_idx`.
    fn read_used_elem(mem: &MockMem, ring_idx: u16) -> (u32, u32) {
        let offset = USED_BASE + 4 + u64::from(ring_idx) * 8;
        let mut id_buf = [0u8; 4];
        let mut len_buf = [0u8; 4];
        mem.read_guest(offset, &mut id_buf)
            .expect("device lock poisoned");
        mem.read_guest(offset + 4, &mut len_buf)
            .expect("device lock poisoned");
        (u32::from_le_bytes(id_buf), u32::from_le_bytes(len_buf))
    }

    fn make_ready_queue(mem: &MockMem) -> VirtQueue {
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);
        // Initialize avail.idx to 0.
        set_avail_idx(mem, 0);
        // Initialize used.idx to 0.
        mem.write_bytes(USED_BASE + 2, &0u16.to_le_bytes());
        q
    }

    #[test]
    fn pop_empty_queue_returns_none() {
        let mem = MockMem::new(0x4000);
        let mut q = make_ready_queue(&mem);
        assert!(q.pop_chain(&mem).is_none());
    }

    #[test]
    fn pop_not_ready_returns_none() {
        let mem = MockMem::new(0x4000);
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        // Queue not marked ready.
        set_avail_idx(&mem, 1);
        assert!(q.pop_chain(&mem).is_none());
    }

    #[test]
    fn pop_single_descriptor() {
        let mem = MockMem::new(0x4000);
        let mut q = make_ready_queue(&mem);

        // Set up one descriptor: addr=0x5000, len=512, no flags, no next.
        write_desc(&mem, 0, 0x5000, 512, 0, 0);
        // Available ring: entry 0 points to descriptor 0.
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let mut chain = q.pop_chain(&mem).expect("should get a chain");
        assert_eq!(chain.head_index(), 0);

        let desc = chain.next_descriptor(&mem).expect("should get descriptor");
        assert_eq!(desc.gpa, 0x5000);
        assert_eq!(desc.len, 512);
        assert!(!desc.is_device_writable);

        // No more descriptors.
        assert!(chain.next_descriptor(&mem).is_none());

        // Queue should now be empty.
        assert!(q.pop_chain(&mem).is_none());
    }

    #[test]
    fn pop_chained_three_descriptors() {
        let mem = MockMem::new(0x4000);
        let mut q = make_ready_queue(&mem);

        // Chain: desc 0 -> desc 1 -> desc 2
        write_desc(&mem, 0, 0x5000, 16, VIRTQ_DESC_F_NEXT, 1); // readable, chains to 1
        write_desc(
            &mem,
            1,
            0x6000,
            512,
            VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
            2,
        ); // writable, chains to 2
        write_desc(&mem, 2, 0x7000, 1, VIRTQ_DESC_F_WRITE, 0); // writable, end of chain

        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let mut chain = q.pop_chain(&mem).expect("should get a chain");
        assert_eq!(chain.head_index(), 0);

        let d0 = chain.next_descriptor(&mem).expect("desc 0");
        assert_eq!(d0.gpa, 0x5000);
        assert_eq!(d0.len, 16);
        assert!(!d0.is_device_writable);

        let d1 = chain.next_descriptor(&mem).expect("desc 1");
        assert_eq!(d1.gpa, 0x6000);
        assert_eq!(d1.len, 512);
        assert!(d1.is_device_writable);

        let d2 = chain.next_descriptor(&mem).expect("desc 2");
        assert_eq!(d2.gpa, 0x7000);
        assert_eq!(d2.len, 1);
        assert!(d2.is_device_writable);

        assert!(chain.next_descriptor(&mem).is_none());
    }

    #[test]
    fn push_used_updates_ring() {
        let mem = MockMem::new(0x4000);
        let q = make_ready_queue(&mem);

        // Push two used entries.
        q.push_used(&mem, 3, 512);
        q.push_used(&mem, 7, 1024);

        assert_eq!(read_used_idx(&mem), 2);

        let (id0, len0) = read_used_elem(&mem, 0);
        assert_eq!(id0, 3);
        assert_eq!(len0, 512);

        let (id1, len1) = read_used_elem(&mem, 1);
        assert_eq!(id1, 7);
        assert_eq!(len1, 1024);
    }

    #[test]
    fn pop_and_push_roundtrip() {
        let mem = MockMem::new(0x4000);
        let mut q = make_ready_queue(&mem);

        write_desc(&mem, 0, 0x5000, 256, 0, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let chain = q.pop_chain(&mem).expect("should get chain");
        let head = chain.head_index();

        q.push_used(&mem, head, 256);

        assert_eq!(read_used_idx(&mem), 1);
        let (id, len) = read_used_elem(&mem, 0);
        assert_eq!(id, 0);
        assert_eq!(len, 256);
    }

    #[test]
    fn queue_reset() {
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);

        q.reset();

        assert!(!q.is_ready());
        assert_eq!(q.size(), MAX_QUEUE_SIZE);
    }

    #[test]
    fn set_size_clamps() {
        let mut q = VirtQueue::new(128);
        assert_eq!(q.size(), 128);

        q.set_size(64);
        assert_eq!(q.size(), 64);

        // 0 is rejected.
        q.set_size(0);
        assert_eq!(q.size(), 64);

        // Over max is rejected.
        q.set_size(512);
        assert_eq!(q.size(), 64);
    }

    #[test]
    fn wrapping_avail_idx() {
        let mem = MockMem::new(0x4000);
        let mut q = VirtQueue::new(4); // small queue
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);

        // Initialize used.idx to 0.
        mem.write_bytes(USED_BASE + 2, &0u16.to_le_bytes());

        // Fill and drain queue multiple times to exercise wrapping.
        for i in 0u16..8 {
            write_desc(&mem, i % 4, u64::from(i % 4) + 0x5000, 64, 0, 0);
            write_avail_entry(&mem, i % 4, i % 4);
            set_avail_idx(&mem, i + 1);

            let chain = q.pop_chain(&mem).expect("should pop");
            assert_eq!(chain.head_index(), i % 4);
        }
    }

    #[test]
    fn virtqueue_new_clamps_size() {
        let q1 = VirtQueue::new(0);
        assert_eq!(q1.size(), MAX_QUEUE_SIZE);

        let q2 = VirtQueue::new(MAX_QUEUE_SIZE + 1);
        assert_eq!(q2.size(), MAX_QUEUE_SIZE);

        let q3 = VirtQueue::new(128);
        assert_eq!(q3.size(), 128);
    }

    #[test]
    fn pop_chain_avail_idx_read_fails() {
        // Create memory too small to read avail_idx (at AVAIL_BASE + 2)
        let mem = MockMem::new(0x1000); // 4096 bytes, AVAIL_BASE is 0x1000
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);

        assert!(q.pop_chain(&mem).is_none());
    }

    #[test]
    fn pop_chain_head_idx_read_fails() {
        // Memory big enough for avail_idx but too small for avail_ring entry
        let mem = MockMem::new(0x1004); // Can read AVAIL_BASE + 2, but not + 4
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);
        // We write directly to the buffer because set_avail_idx expects mem to be big enough
        mem.inner.lock().expect("device lock poisoned")[0x1002..0x1004]
            .copy_from_slice(&1u16.to_le_bytes());

        assert!(q.pop_chain(&mem).is_none());
    }

    #[test]
    fn next_descriptor_read_fails() {
        // Set up avail ring to point to a descriptor index that is within queue size
        // but its backing memory is intentionally unreadable or partially mapped
        // Here we just make memory small.
        let small_mem = MockMem::new(0x1004); // Can read avail index but not descriptor base
        small_mem.inner.lock().expect("device lock poisoned")[0x1002..0x1004]
            .copy_from_slice(&1u16.to_le_bytes());
        // Head index 0 at offset + 4
        small_mem
            .inner
            .lock()
            .expect("device lock poisoned")
            .resize(0x1006, 0); // Need to read head index 0 at 0x1004
        small_mem.inner.lock().expect("device lock poisoned")[0x1004..0x1006]
            .copy_from_slice(&0u16.to_le_bytes());

        let mut q2 = VirtQueue::new(16);
        q2.configure(0x2000, AVAIL_BASE, USED_BASE); // DESC_BASE is 0x2000, beyond memory size
        q2.set_ready(true);

        let mut chain = q2.pop_chain(&small_mem).expect("should pop chain");
        assert!(chain.next_descriptor(&small_mem).is_none());
    }

    #[test]
    fn next_descriptor_out_of_bounds_idx() {
        let mem = MockMem::new(0x4000);
        let mut q = make_ready_queue(&mem);

        // Chain points to an invalid descriptor index >= queue_size (16)
        write_desc(&mem, 0, 0x5000, 16, VIRTQ_DESC_F_NEXT, 32); // 32 >= 16

        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let mut chain = q.pop_chain(&mem).expect("should pop chain");
        assert!(chain.next_descriptor(&mem).is_some()); // first desc reads fine
        assert!(chain.next_descriptor(&mem).is_none()); // second desc fails bounds check
    }

    #[test]
    fn push_used_idx_read_fails() {
        // Memory too small to read used_idx at USED_BASE + 2
        let mem = MockMem::new(0x2000); // Only goes up to 0x1FFF, USED_BASE is 0x2000
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);

        // Call push_used. It shouldn't panic, but rather return early.
        q.push_used(&mem, 0, 512);
    }

    #[test]
    fn havoc_cyclic_descriptor_chain_aborts() {
        let mem = MockMem::new(0x4000);
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);

        // Create a cycle: 0 -> 1 -> 0
        write_desc(&mem, 0, 0x100, 10, VIRTQ_DESC_F_NEXT, 1);
        write_desc(&mem, 1, 0x200, 20, VIRTQ_DESC_F_NEXT, 0);

        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let mut chain = q.pop_chain(&mem).expect("should pop chain");

        let mut count = 0;
        while let Some(_desc) = chain.next_descriptor(&mem) {
            count += 1;
            // Failsafe in case the bug is present
            assert!(count <= 100, "Infinite loop detected!");
        }

        // It should process exactly 16 descriptors (the queue size) before aborting
        assert_eq!(count, 16);
    }

    #[test]
    fn should_clamp_virtqueue_size_to_max_when_zero() {
        let queue = VirtQueue::new(0);
        assert_eq!(
            queue.size, MAX_QUEUE_SIZE,
            "VirtQueue initialized with size 0 must be clamped to MAX_QUEUE_SIZE to prevent modulo-by-zero panics in push_used"
        );
    }

    #[test]
    fn should_clamp_virtqueue_size_to_max_when_exceeding_limit() {
        let queue = VirtQueue::new(MAX_QUEUE_SIZE + 1);
        assert_eq!(
            queue.size, MAX_QUEUE_SIZE,
            "VirtQueue initialized with size > MAX_QUEUE_SIZE must be clamped"
        );
    }
}

#[cfg(test)]
mod queue_test;
