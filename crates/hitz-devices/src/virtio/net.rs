//! Virtio network device backend.
//!
//! Implements [`VirtioBackend`] for network I/O. Frames are exchanged
//! with an I/O thread via crossbeam channels. The device has two
//! virtqueues: RX (queue 0) and TX (queue 1).
//!
//! Device ID: 1 (virtio net device).

use std::collections::VecDeque;

use crossbeam_channel::{Receiver, Sender};
use hitz_hal::GuestMemAccess;

use crate::virtio::mmio_transport::VirtioBackend;
use crate::virtio::queue::VirtQueue;

/// Size of the virtio-net header prepended to every frame.
const VIRTIO_NET_HDR_SIZE: usize = 12;

/// Feature bit: device has a given MAC address in config space.
const VIRTIO_NET_F_MAC: u64 = 1 << 5;

/// RX virtqueue index (guest receives frames here).
const RX_QUEUE: u16 = 0;

/// TX virtqueue index (guest sends frames here).
const TX_QUEUE: u16 = 1;

/// A virtio-net device implementation.
///
/// # Abstract
///
/// Provides a paravirtualized network interface to the guest OS. It uses two
/// virtqueues: one for receiving packets (RX) and one for transmitting packets (TX).
///
/// This structure bridges the guest's memory buffers with host-side crossbeam
/// channels, allowing an asynchronous host network thread to inject received ethernet
/// frames and drain transmitted frames.
///
/// # The Hero's Journey
///
/// ```no_run
/// # use hitz_devices::VirtioNetDevice;
/// # use crossbeam_channel::unbounded;
/// # use hitz_devices::VirtioMmioTransport;
/// # use hitz_hal::GuestMemAccess;
/// # use std::sync::Arc;
/// # struct DummyMem;
/// # impl GuestMemAccess for DummyMem {
/// #     fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// #     fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
/// # }
/// // 1. Create the virtio-net device.
/// let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
/// let (net_device, host_rx, host_tx) = VirtioNetDevice::new(mac);
///
/// // 2. Wrap it in a virtio-mmio transport and register it to the MmioBus.
/// let mem = Arc::new(DummyMem);
/// let transport = VirtioMmioTransport::new(net_device, mem, 5);
/// ```
///
/// # The Fine Print
///
/// - Requires `VIRTIO_F_VERSION_1` and implements the `VIRTIO_NET_F_MAC` feature.
/// - The RX and TX queues must be polled periodically on the `MmioBus` via `poll_rx`.
pub struct VirtioNetDevice {
    /// The device's MAC address (reported in config space).
    mac: [u8; 6],
    /// Channel to send TX frames (raw Ethernet, no virtio-net header) to the I/O thread.
    tx_sender: Sender<Vec<u8>>,
    /// Channel to receive RX frames from the I/O thread.
    rx_receiver: Receiver<Vec<u8>>,
    /// Frames waiting to be delivered to the guest's RX queue.
    rx_pending: VecDeque<Vec<u8>>,
}

impl VirtioNetDevice {
    /// Create a new virtio-net device with the given MAC address.
    ///
    /// Returns:
    /// - The device itself
    /// - A `Receiver<Vec<u8>>` for the I/O thread to read TX frames from the guest
    /// - A `Sender<Vec<u8>>` for the I/O thread to inject RX frames into the guest
    #[must_use]
    pub fn new(mac: [u8; 6]) -> (Self, Receiver<Vec<u8>>, Sender<Vec<u8>>) {
        let (tx_sender, tx_receiver) = crossbeam_channel::unbounded();
        let (rx_sender, rx_receiver) = crossbeam_channel::unbounded();

        let device = Self {
            mac,
            tx_sender,
            rx_receiver,
            rx_pending: VecDeque::new(),
        };

        (device, tx_receiver, rx_sender)
    }

    /// Process a TX descriptor chain: read the guest's outgoing frame,
    /// strip the virtio-net header, and send the raw Ethernet frame
    /// to the I/O thread.
    fn process_tx(&self, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        // ⚡ Bolt: Hoist buffer to eliminate multiple heap allocations per transmitted frame.
        let mut frame_data = Vec::with_capacity(2048);
        while let Some(mut chain) = queue.pop_chain(mem) {
            let head = chain.head_index();
            frame_data.clear();

            // Collect all device-readable buffers in the chain.
            while let Some(desc) = chain.next_descriptor(mem) {
                if !desc.is_device_writable {
                    let start_len = frame_data.len();
                    if start_len.saturating_add(desc.len as usize) > 65536 {
                        break; // Prevent unbounded memory allocation from massive descriptors
                    }
                    frame_data.resize(start_len + desc.len as usize, 0);
                    if mem
                        .read_guest(desc.gpa, &mut frame_data[start_len..])
                        .is_err()
                    {
                        frame_data.truncate(start_len); // Revert on read failure
                    }
                }
            }

            // Strip the virtio-net header (first 12 bytes).
            if frame_data.len() > VIRTIO_NET_HDR_SIZE {
                let ethernet_frame = frame_data[VIRTIO_NET_HDR_SIZE..].to_vec();
                // Best-effort send: if the I/O thread disconnected, drop the frame.
                let _ = self.tx_sender.send(ethernet_frame);
            }

            queue.push_used(mem, head, 0);
        }
    }

    /// Deliver one pending RX frame to the guest's RX queue.
    ///
    /// Prepends a 12-byte all-zeros virtio-net header, then writes
    /// the frame into device-writable descriptors.
    ///
    /// Returns `true` if a frame was delivered, `false` if no chain
    /// was available or no frames were pending.
    fn deliver_rx(&mut self, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) -> bool {
        let Some(raw_frame) = self.rx_pending.front() else {
            return false;
        };

        let Some(mut chain) = queue.pop_chain(mem) else {
            return false;
        };

        // Build the full frame: virtio-net header (12 zero bytes) + raw Ethernet.
        let mut payload = vec![0u8; VIRTIO_NET_HDR_SIZE];
        payload.extend_from_slice(raw_frame);

        let head = chain.head_index();
        let mut total_written: u32 = 0;
        let mut offset = 0usize;

        // Write into device-writable descriptors.
        while let Some(desc) = chain.next_descriptor(mem) {
            if desc.is_device_writable && offset < payload.len() {
                let remaining = payload.len() - offset;
                let to_write = remaining.min(desc.len as usize);
                if mem
                    .write_guest(desc.gpa, &payload[offset..offset + to_write])
                    .is_ok()
                {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        total_written += to_write as u32;
                    }
                    offset += to_write;
                }
            }
        }

        queue.push_used(mem, head, total_written);

        // Only dequeue the frame if we actually wrote something.
        if total_written > 0 {
            let _ = self.rx_pending.pop_front();
        }

        total_written > 0
    }
}

impl VirtioBackend for VirtioNetDevice {
    fn device_id(&self) -> u32 {
        1 // virtio net
    }

    fn device_features(&self) -> u64 {
        VIRTIO_NET_F_MAC
    }

    fn queue_count(&self) -> usize {
        2
    }

    fn process_queue(&mut self, queue_idx: u16, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        match queue_idx {
            TX_QUEUE => self.process_tx(queue, mem),
            RX_QUEUE => {
                let _ = self.deliver_rx(queue, mem);
            }
            _ => {
                tracing::debug!(queue_idx, "virtio-net: unknown queue index");
            }
        }
    }

    fn poll_rx(&mut self, rx_queue: &mut VirtQueue, mem: &dyn GuestMemAccess) -> bool {
        // Drain the channel into rx_pending.
        while let Ok(frame) = self.rx_receiver.try_recv() {
            self.rx_pending.push_back(frame);
        }

        // Try to deliver frames.
        self.deliver_rx(rx_queue, mem)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space layout: MAC address at offset 0 (6 bytes).
        let start = offset as usize;
        if start < 6 {
            let end = (start + data.len()).min(6);
            let len = end - start;
            data[..len].copy_from_slice(&self.mac[start..end]);
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // MAC address is read-only in config space.
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Minimal no-op guest memory for tests that don't need real memory.
    struct NoMem;

    impl GuestMemAccess for NoMem {
        fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }

        fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), hitz_hal::HalError> {
            Ok(())
        }
    }

    #[test]
    fn device_id_is_network() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, _tx_rx, _rx_tx) = VirtioNetDevice::new(mac);
        assert_eq!(dev.device_id(), 1);
    }

    #[test]
    fn queue_count_is_two() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, _tx_rx, _rx_tx) = VirtioNetDevice::new(mac);
        assert_eq!(dev.queue_count(), 2);
    }

    #[test]
    fn features_include_mac() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, _tx_rx, _rx_tx) = VirtioNetDevice::new(mac);
        assert_ne!(dev.device_features() & VIRTIO_NET_F_MAC, 0);
    }

    #[test]
    fn config_space_returns_mac() {
        let mac = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let (dev, _tx_rx, _rx_tx) = VirtioNetDevice::new(mac);

        let mut buf = [0u8; 6];
        dev.read_config(0, &mut buf);
        assert_eq!(buf, mac);
    }

    #[test]
    fn config_space_partial_read() {
        let mac = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let (dev, _tx_rx, _rx_tx) = VirtioNetDevice::new(mac);

        // Read just 2 bytes from offset 2.
        let mut buf = [0u8; 2];
        dev.read_config(2, &mut buf);
        assert_eq!(buf, [0xBB, 0xCC]);
    }

    #[test]
    fn tx_frame_reaches_io_thread() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, tx_receiver, _rx_sender) = VirtioNetDevice::new(mac);

        // Simulate a TX by directly using the internal tx_sender.
        // In real usage, process_tx would call this after walking descriptors.
        let test_frame = vec![0xFF; 64];
        dev.tx_sender
            .send(test_frame.clone())
            .expect("send should succeed");

        let received = tx_receiver.recv().expect("should receive frame");
        assert_eq!(received, test_frame);
    }

    #[test]
    fn rx_frame_queued_from_io_thread() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (mut dev, _tx_receiver, rx_sender) = VirtioNetDevice::new(mac);

        // I/O thread sends a frame.
        let test_frame = vec![0xAB; 64];
        rx_sender
            .send(test_frame.clone())
            .expect("send should succeed");

        // Create a dummy VirtQueue that's not ready -- pop_chain will return None,
        // so deliver_rx won't actually deliver, but poll_rx should drain the channel
        // into rx_pending.
        let mut dummy_queue = VirtQueue::new(128);
        let mem = NoMem;

        let delivered = dev.poll_rx(&mut dummy_queue, &mem);
        // No chain available so deliver fails.
        assert!(!delivered);

        // But the frame should be in rx_pending.
        assert_eq!(dev.rx_pending.len(), 1);
        assert_eq!(dev.rx_pending[0], test_frame);
    }

    use std::sync::Mutex;
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
            let mut mem = self.inner.lock().expect("Mutex poisoned");
            let start = offset as usize;
            if start + data.len() <= mem.len() {
                mem[start..start + data.len()].copy_from_slice(data);
            }
        }
    }
    impl GuestMemAccess for MockMem {
        #[allow(clippy::significant_drop_tightening)]
        fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), hitz_hal::HalError> {
            let mem = self.inner.lock().expect("Mutex poisoned");
            let start = gpa as usize;
            if start + buf.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: buf.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            buf.copy_from_slice(&mem[start..start + buf.len()]);
            Ok(())
        }
        #[allow(clippy::significant_drop_tightening)]
        fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), hitz_hal::HalError> {
            let mut mem = self.inner.lock().expect("Mutex poisoned");
            let start = gpa as usize;
            if start + data.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: data.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            mem[start..start + data.len()].copy_from_slice(data);
            Ok(())
        }
    }

    fn write_desc(mem: &MockMem, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = u64::from(idx) * 16;
        mem.write_bytes(offset, &addr.to_le_bytes());
        mem.write_bytes(offset + 8, &len.to_le_bytes());
        mem.write_bytes(offset + 12, &flags.to_le_bytes());
        mem.write_bytes(offset + 14, &next.to_le_bytes());
    }

    fn write_avail_entry(mem: &MockMem, ring_idx: u16, desc_idx: u16) {
        let offset = 0x1000 + 4 + u64::from(ring_idx) * 2;
        mem.write_bytes(offset, &desc_idx.to_le_bytes());
    }

    fn set_avail_idx(mem: &MockMem, idx: u16) {
        mem.write_bytes(0x1000 + 2, &idx.to_le_bytes());
    }

    #[test]
    fn havoc_net_tx_oom() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, _tx_receiver, _rx_sender) = VirtioNetDevice::new(mac);

        let mem = MockMem::new(0x10000);
        let mut q = VirtQueue::new(16);
        q.configure(0, 0x1000, 0x2000);
        q.set_ready(true);
        set_avail_idx(&mem, 0);
        mem.write_bytes(0x2000 + 2, &0u16.to_le_bytes());

        // Create a massive desc
        write_desc(&mem, 0, 0x4000, u32::MAX, 0, 0); // VIRTQ_DESC_F_WRITE = 2, so 0 is readable
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_tx(&mut q, &mem);
    }

    #[test]
    fn should_handle_out_of_bounds_read_in_process_tx() {
        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (dev, _tx_receiver, _rx_sender) = VirtioNetDevice::new(mac);

        let mem = MockMem::new(0x10000);
        let mut q = VirtQueue::new(16);
        q.configure(0, 0x1000, 0x2000);
        q.set_ready(true);
        set_avail_idx(&mem, 0);
        mem.write_bytes(0x2000 + 2, &0u16.to_le_bytes());

        // Valid desc length, but address is out of bounds (0x20000 > 0x10000)
        write_desc(&mem, 0, 0x20000, 32, 0, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        // This should not panic; it will just fail to read the memory and return early or drop the frame
        dev.process_tx(&mut q, &mem);
    }

    #[test]
    fn should_handle_out_of_bounds_write_in_deliver_rx() {
        // Create a writable desc pointing out of bounds (0x20000 > 0x10000)
        const F_WRITE: u16 = 2;

        let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let (mut dev, _tx_receiver, rx_sender) = VirtioNetDevice::new(mac);

        // Inject a pending RX frame
        let test_frame = vec![0xAB; 64];
        rx_sender.send(test_frame).unwrap();

        let mem = MockMem::new(0x10000);
        let mut q = VirtQueue::new(16);
        q.configure(0, 0x1000, 0x2000);
        q.set_ready(true);
        set_avail_idx(&mem, 0);
        mem.write_bytes(0x2000 + 2, &0u16.to_le_bytes());

        write_desc(&mem, 0, 0x20000, 128, F_WRITE, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        // Process queue to drain rx_sender and call deliver_rx
        let _ = dev.poll_rx(&mut q, &mem);

        // rx_pending should still have the frame since delivery failed
        assert_eq!(dev.rx_pending.len(), 1);
    }
}
