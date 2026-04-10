//! Virtio-vsock device (device ID 19) — packet header and host-side device.
//!
//! Reference: virtio spec 1.2, section 5.10.
//! Linux kernel: `include/uapi/linux/virtio_vsock.h`

use std::collections::VecDeque;

use crossbeam_channel::{Receiver, Sender};

use crate::virtio::mmio_transport::VirtioBackend;
use crate::virtio::queue::VirtQueue;

// ── Protocol constants ────────────────────────────────────────────────────────

/// Size of the virtio-vsock packet header in bytes (44 bytes).
pub const VSOCK_HDR_SIZE: usize = 44;

/// Stream socket type (the only type we implement).
pub const VSOCK_TYPE_STREAM: u16 = 1;

/// Host CID (as defined by the virtio-vsock spec).
pub const VMADDR_CID_HOST: u64 = 2;

/// Initial receive buffer size advertised per connection (256 KiB).
pub const VSOCK_BUF_ALLOC: u32 = 256 * 1024;

// ── VsockOp ──────────────────────────────────────────────────────────────────

/// Virtio-vsock packet operations.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsockOp {
    /// Guest requests a connection.
    Request = 1,
    /// Host accepts a connection.
    Response = 2,
    /// Reset / reject a connection.
    Rst = 3,
    /// Shutdown one or both directions.
    Shutdown = 4,
    /// Data transfer.
    Rw = 5,
    /// Flow-control credit update.
    CreditUpdate = 6,
    /// Request a credit update from the peer.
    CreditRequest = 7,
}

impl VsockOp {
    /// Parse a raw u16 into a `VsockOp`. Returns `None` for unknown values.
    #[must_use]
    pub const fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Self::Request),
            2 => Some(Self::Response),
            3 => Some(Self::Rst),
            4 => Some(Self::Shutdown),
            5 => Some(Self::Rw),
            6 => Some(Self::CreditUpdate),
            7 => Some(Self::CreditRequest),
            _ => None,
        }
    }
}

// ── VsockHdr ─────────────────────────────────────────────────────────────────

/// Virtio-vsock packet header (44 bytes, little-endian).
///
/// Follows `struct virtio_vsock_hdr` from the Linux kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VsockHdr {
    /// Source context ID.
    pub src_cid: u64,
    /// Destination context ID.
    pub dst_cid: u64,
    /// Source port.
    pub src_port: u32,
    /// Destination port.
    pub dst_port: u32,
    /// Payload length in bytes (not including this header).
    pub len: u32,
    /// Always `VSOCK_TYPE_STREAM`.
    pub r#type: u16,
    /// Operation (see [`VsockOp`]).
    pub op: u16,
    /// Flags (used for SHUTDOWN direction bits).
    pub flags: u32,
    /// Receiver's total buffer allocation (flow control).
    pub buf_alloc: u32,
    /// Bytes consumed by receiver so far (flow control).
    pub fwd_cnt: u32,
}

impl VsockHdr {
    /// Serialize header to 44 little-endian bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; VSOCK_HDR_SIZE] {
        let mut b = [0u8; VSOCK_HDR_SIZE];
        b[0..8].copy_from_slice(&self.src_cid.to_le_bytes());
        b[8..16].copy_from_slice(&self.dst_cid.to_le_bytes());
        b[16..20].copy_from_slice(&self.src_port.to_le_bytes());
        b[20..24].copy_from_slice(&self.dst_port.to_le_bytes());
        b[24..28].copy_from_slice(&self.len.to_le_bytes());
        b[28..30].copy_from_slice(&self.r#type.to_le_bytes());
        b[30..32].copy_from_slice(&self.op.to_le_bytes());
        b[32..36].copy_from_slice(&self.flags.to_le_bytes());
        b[36..40].copy_from_slice(&self.buf_alloc.to_le_bytes());
        b[40..44].copy_from_slice(&self.fwd_cnt.to_le_bytes());
        b
    }

    /// Deserialize header from 44 little-endian bytes.
    ///
    /// Returns `None` if `bytes.len() < 44`.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < VSOCK_HDR_SIZE {
            return None;
        }
        Some(Self {
            src_cid: u64::from_le_bytes(bytes[0..8].try_into().ok()?),
            dst_cid: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
            src_port: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
            dst_port: u32::from_le_bytes(bytes[20..24].try_into().ok()?),
            len: u32::from_le_bytes(bytes[24..28].try_into().ok()?),
            r#type: u16::from_le_bytes(bytes[28..30].try_into().ok()?),
            op: u16::from_le_bytes(bytes[30..32].try_into().ok()?),
            flags: u32::from_le_bytes(bytes[32..36].try_into().ok()?),
            buf_alloc: u32::from_le_bytes(bytes[36..40].try_into().ok()?),
            fwd_cnt: u32::from_le_bytes(bytes[40..44].try_into().ok()?),
        })
    }

    /// Build a response header (swaps src/dst, sets the given op).
    #[must_use]
    pub const fn make_response(&self, op: VsockOp, payload_len: u32) -> Self {
        Self {
            src_cid: self.dst_cid,
            dst_cid: self.src_cid,
            src_port: self.dst_port,
            dst_port: self.src_port,
            len: payload_len,
            r#type: VSOCK_TYPE_STREAM,
            op: op as u16,
            flags: 0,
            buf_alloc: VSOCK_BUF_ALLOC,
            fwd_cnt: 0,
        }
    }
}

// ── Channel type alias ────────────────────────────────────────────────────────

/// A vsock packet: header plus raw payload bytes.
pub type VsockPacket = (VsockHdr, Vec<u8>);

// ── VirtioVsockDevice ─────────────────────────────────────────────────────────

/// Virtio-vsock device (device ID 19).
///
/// Bridges the guest virtio driver with host-side socket handling.
/// Packets from the guest arrive on the TX queue (queue 1); packets
/// to the guest are injected via the RX queue (queue 0).
///
/// The event queue (queue 2) is stubbed — descriptors are immediately
/// returned unused, which satisfies the Linux driver.
///
/// Use [`VirtioVsockDevice::new`] to construct; the returned channel
/// endpoints connect to the host-side async runtime.
pub struct VirtioVsockDevice {
    /// Guest CID assigned to this VM.
    guest_cid: u64,
    /// Send guest→host packets (TX queue contents) to the host runtime.
    tx_sender: Sender<VsockPacket>,
    /// Receive host→guest packets (to inject into RX queue).
    rx_receiver: Receiver<VsockPacket>,
    /// Pending packets waiting to be written into the RX virtqueue.
    rx_pending: VecDeque<VsockPacket>,
}

impl VirtioVsockDevice {
    /// Create a new vsock device for the given guest CID.
    ///
    /// Returns:
    /// - The device (for the MMIO transport in the vCPU thread)
    /// - A `Receiver` for the host runtime to read TX packets from the guest
    /// - A `Sender` for the host runtime to inject RX packets into the guest
    #[must_use]
    pub fn new(guest_cid: u32) -> (Self, Receiver<VsockPacket>, Sender<VsockPacket>) {
        let (tx_sender, tx_receiver) = crossbeam_channel::unbounded();
        let (rx_sender, rx_receiver) = crossbeam_channel::unbounded();
        let device = Self {
            guest_cid: u64::from(guest_cid),
            tx_sender,
            rx_receiver,
            rx_pending: VecDeque::new(),
        };
        (device, tx_receiver, rx_sender)
    }

    /// Create a vsock device from pre-existing channel endpoints.
    ///
    /// Use this when the host-side async runtime has already created the
    /// channel pairs and holds the opposite ends. This avoids the need to
    /// extract channels after `spawn_blocking` has taken ownership.
    ///
    /// - `rx_receiver`: device reads from this to inject host→guest packets
    /// - `tx_sender`: device writes to this to forward guest→host packets
    #[must_use]
    pub fn with_channels(
        guest_cid: u32,
        rx_receiver: Receiver<VsockPacket>,
        tx_sender: Sender<VsockPacket>,
    ) -> Self {
        Self {
            guest_cid: u64::from(guest_cid),
            tx_sender,
            rx_receiver,
            rx_pending: VecDeque::new(),
        }
    }

    /// Process the TX virtqueue: read all pending guest→host packets and
    /// forward them to the host via `tx_sender`.
    fn process_tx(&self, queue: &mut VirtQueue, mem: &dyn hitz_hal::GuestMemAccess) {
        // ⚡ Bolt: Hoist buffer to eliminate multiple heap allocations per transmitted packet.
        let mut raw = Vec::with_capacity(1024);
        while let Some(mut chain) = queue.pop_chain(mem) {
            let head = chain.head_index();
            raw.clear();

            while let Some(desc) = chain.next_descriptor(mem) {
                if desc.is_device_writable {
                    break; // TX descriptors are all device-readable
                }
                let start_len = raw.len();
                raw.resize(start_len + desc.len as usize, 0);
                if mem.read_guest(desc.gpa, &mut raw[start_len..]).is_err() {
                    raw.truncate(start_len); // Revert on read failure
                }
            }

            if let Some(hdr) = (raw.len() >= VSOCK_HDR_SIZE)
                .then(|| VsockHdr::from_bytes(&raw))
                .flatten()
            {
                let payload_end = VSOCK_HDR_SIZE + hdr.len as usize;
                let payload = raw.get(VSOCK_HDR_SIZE..payload_end).unwrap_or(&[]).to_vec();
                // Non-blocking send; if channel is closed, drop packet.
                let _ = self.tx_sender.try_send((hdr, payload));
            }

            queue.push_used(mem, head, 0);
        }
    }

    /// Stub the event virtqueue: return all pending descriptors unused.
    /// The Linux vsock driver expects descriptors it posted to come back.
    fn process_event(queue: &mut VirtQueue, mem: &dyn hitz_hal::GuestMemAccess) {
        while let Some(chain) = queue.pop_chain(mem) {
            queue.push_used(mem, chain.head_index(), 0);
        }
    }
}

impl VirtioBackend for VirtioVsockDevice {
    fn device_id(&self) -> u32 {
        19 // VIRTIO_ID_VSOCK
    }

    fn device_features(&self) -> u64 {
        0 // No optional features
    }

    fn queue_count(&self) -> usize {
        3 // RX=0, TX=1, Event=2
    }

    fn process_queue(
        &mut self,
        queue_idx: u16,
        queue: &mut VirtQueue,
        mem: &dyn hitz_hal::GuestMemAccess,
    ) {
        match queue_idx {
            0 => { /* RX: guest posts receive buffers, no action needed */ }
            1 => self.process_tx(queue, mem),
            2 => Self::process_event(queue, mem),
            _ => tracing::warn!(queue_idx, "vsock: unexpected queue notify"),
        }
    }

    fn poll_rx(&mut self, rx_queue: &mut VirtQueue, mem: &dyn hitz_hal::GuestMemAccess) -> bool {
        // Drain channel into local pending queue.
        while let Ok(packet) = self.rx_receiver.try_recv() {
            self.rx_pending.push_back(packet);
        }

        if self.rx_pending.is_empty() {
            return false;
        }

        let mut injected = false;

        while let Some((hdr, payload)) = self.rx_pending.pop_front() {
            // Try to inject one packet into the RX virtqueue.
            let Some(mut chain) = rx_queue.pop_chain(mem) else {
                // No RX buffers available — push packet back.
                self.rx_pending.push_front((hdr, payload));
                break;
            };

            let head = chain.head_index();
            let hdr_bytes = hdr.to_bytes();
            let total = hdr_bytes.len() + payload.len();

            // Find a writable descriptor to write into.
            let Some(desc) = chain.next_descriptor(mem).filter(|d| d.is_device_writable) else {
                // Couldn't write — push packet back for next poll.
                self.rx_pending.push_front((hdr, payload));
                rx_queue.push_used(mem, head, 0);
                break;
            };

            let _ = mem.write_guest(desc.gpa, &hdr_bytes);
            if !payload.is_empty() {
                let payload_addr =
                    desc.gpa + u64::try_from(hdr_bytes.len()).unwrap_or(VSOCK_HDR_SIZE as u64);
                let _ = mem.write_guest(payload_addr, &payload);
            }
            rx_queue.push_used(mem, head, u32::try_from(total).unwrap_or(u32::MAX));
            injected = true;
        }

        injected
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space: guest_cid at offset 0 (8 bytes, little-endian).
        let cid_bytes = self.guest_cid.to_le_bytes();
        let Ok(src_start) = usize::try_from(offset) else {
            return;
        };
        let src_end = (src_start + data.len()).min(cid_bytes.len());
        if src_start < cid_bytes.len() {
            let len = src_end - src_start;
            data[..len].copy_from_slice(&cid_bytes[src_start..src_end]);
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // Guest cannot change the CID.
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn vsock_hdr_roundtrip() {
        let hdr = VsockHdr {
            src_cid: 3,
            dst_cid: 2,
            src_port: 12345,
            dst_port: 52355,
            len: 42,
            r#type: VSOCK_TYPE_STREAM,
            op: VsockOp::Rw as u16,
            flags: 0,
            buf_alloc: 262_144,
            fwd_cnt: 0,
        };
        let bytes = hdr.to_bytes();
        assert_eq!(bytes.len(), VSOCK_HDR_SIZE);
        let parsed = VsockHdr::from_bytes(&bytes).expect("parse");
        assert_eq!(parsed.src_cid, 3);
        assert_eq!(parsed.dst_cid, 2);
        assert_eq!(parsed.len, 42);
        assert_eq!(parsed.op, VsockOp::Rw as u16);
    }

    #[test]
    fn vsock_op_known_values() {
        assert_eq!(VsockOp::Request as u16, 1);
        assert_eq!(VsockOp::Response as u16, 2);
        assert_eq!(VsockOp::Rst as u16, 3);
        assert_eq!(VsockOp::Shutdown as u16, 4);
        assert_eq!(VsockOp::Rw as u16, 5);
        assert_eq!(VsockOp::CreditUpdate as u16, 6);
        assert_eq!(VsockOp::CreditRequest as u16, 7);
    }

    #[test]
    fn vsock_device_construction() {
        let (device, _rx_recv, _tx_send) = VirtioVsockDevice::new(3);
        assert_eq!(device.device_id(), 19);
        assert_eq!(device.queue_count(), 3);
    }

    #[test]
    fn vsock_config_returns_guest_cid() {
        let (device, _rx_recv, _tx_send) = VirtioVsockDevice::new(42);
        let mut buf = [0u8; 8];
        device.read_config(0, &mut buf);
        let cid = u64::from_le_bytes(buf);
        assert_eq!(cid, 42);
    }

    use hitz_hal::{GuestMemAccess, HalError};
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
            let mut mem = self.inner.lock().unwrap();
            let start = offset as usize;
            mem[start..start + data.len()].copy_from_slice(data);
        }
    }
    impl GuestMemAccess for MockMem {
        #[allow(clippy::significant_drop_tightening)]
        fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), HalError> {
            let mem = self.inner.lock().unwrap();
            let start = gpa as usize;
            if start + buf.len() > mem.len() {
                return Err(HalError::MapMemory {
                    gpa,
                    size: buf.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            buf.copy_from_slice(&mem[start..start + buf.len()]);
            Ok(())
        }

        #[allow(clippy::significant_drop_tightening)]
        fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), HalError> {
            let mut mem = self.inner.lock().unwrap();
            let start = gpa as usize;
            if start + data.len() > mem.len() {
                return Err(HalError::MapMemory {
                    gpa,
                    size: data.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            mem[start..start + data.len()].copy_from_slice(data);
            Ok(())
        }
    }

    const DESC_BASE: u64 = 0x0000;
    const AVAIL_BASE: u64 = 0x1000;
    const USED_BASE: u64 = 0x2000;

    fn write_desc(mem: &MockMem, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = DESC_BASE + u64::from(idx) * 16;
        mem.write_bytes(offset, &addr.to_le_bytes());
        mem.write_bytes(offset + 8, &len.to_le_bytes());
        mem.write_bytes(offset + 12, &flags.to_le_bytes());
        mem.write_bytes(offset + 14, &next.to_le_bytes());
    }

    fn write_avail_entry(mem: &MockMem, ring_idx: u16, desc_idx: u16) {
        let offset = AVAIL_BASE + 4 + u64::from(ring_idx) * 2;
        mem.write_bytes(offset, &desc_idx.to_le_bytes());
    }

    fn set_avail_idx(mem: &MockMem, idx: u16) {
        mem.write_bytes(AVAIL_BASE + 2, &idx.to_le_bytes());
    }

    fn setup_queue(mem: &MockMem) -> VirtQueue {
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);
        set_avail_idx(mem, 0);
        mem.write_bytes(USED_BASE + 2, &0u16.to_le_bytes());
        q
    }

    #[test]
    fn poll_rx_injects_packet() {
        let (mut device, _rx_recv, tx_send) = VirtioVsockDevice::new(3);
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Send packet to rx channel
        let hdr = VsockHdr {
            src_cid: 2,
            dst_cid: 3,
            src_port: 1000,
            dst_port: 2000,
            len: 4,
            r#type: VSOCK_TYPE_STREAM,
            op: VsockOp::Rw as u16,
            flags: 0,
            buf_alloc: 1024,
            fwd_cnt: 0,
        };
        tx_send.send((hdr, vec![1, 2, 3, 4])).unwrap();

        // One writable descriptor
        write_desc(&mem, 0, 0x3000, 1024, 2, 0); // 2 = F_WRITE
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        let injected = device.poll_rx(&mut q, &mem);
        assert!(injected);

        // Check if data is at 0x3000
        let mut out = vec![0u8; VSOCK_HDR_SIZE + 4];
        mem.read_guest(0x3000, &mut out).unwrap();

        let out_hdr = VsockHdr::from_bytes(&out[..VSOCK_HDR_SIZE]).unwrap();
        assert_eq!(out_hdr.src_cid, 2);
        assert_eq!(out_hdr.len, 4);
        assert_eq!(&out[VSOCK_HDR_SIZE..], &[1, 2, 3, 4]);

        // Used index should be 1
        let mut used_idx_buf = [0u8; 2];
        mem.read_guest(USED_BASE + 2, &mut used_idx_buf).unwrap();
        assert_eq!(u16::from_le_bytes(used_idx_buf), 1);
    }

    #[test]
    fn process_tx_extracts_packet() {
        let (mut device, rx_recv, _tx_send) = VirtioVsockDevice::new(3);
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        let hdr = VsockHdr {
            src_cid: 3,
            dst_cid: 2,
            src_port: 2000,
            dst_port: 1000,
            len: 5,
            r#type: VSOCK_TYPE_STREAM,
            op: VsockOp::Rw as u16,
            flags: 0,
            buf_alloc: 1024,
            fwd_cnt: 0,
        };
        let mut packet = hdr.to_bytes().to_vec();
        packet.extend_from_slice(&[10, 20, 30, 40, 50]);

        // One readable descriptor
        mem.write_bytes(0x4000, &packet);
        write_desc(&mem, 0, 0x4000, packet.len() as u32, 0, 0); // F_WRITE = 0
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        device.process_queue(1, &mut q, &mem);

        // Receiver should get packet
        let (out_hdr, out_payload) = rx_recv.try_recv().unwrap();
        assert_eq!(out_hdr.len, 5);
        assert_eq!(out_hdr.dst_cid, 2);
        assert_eq!(out_payload, vec![10, 20, 30, 40, 50]);
    }
}
