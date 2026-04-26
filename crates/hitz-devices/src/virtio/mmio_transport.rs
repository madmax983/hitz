//! Virtio MMIO transport (version 2).
//!
//! # Abstract
//! Implements the `MmioDevice` trait, translating MMIO register reads/writes
//! into virtqueue operations and device config accesses. This is the "glue"
//! between the guest driver and a [`VirtioBackend`].
//!
//! # The Hero's Journey
//! ```
//! # use hitz_devices::VirtioMmioTransport;
//! # use hitz_devices::VirtioBackend;
//! # use std::sync::Arc;
//! # use hitz_hal::GuestMemAccess;
//! # struct DummyBackend;
//! # impl VirtioBackend for DummyBackend {
//! #     fn device_id(&self) -> u32 { 1 }
//! #     fn device_features(&self) -> u64 { 0 }
//! #     fn queue_count(&self) -> usize { 1 }
//!     fn process_queue(&mut self, _idx: u16, _q: &mut hitz_devices::VirtQueue, _mem: &dyn GuestMemAccess) {}
//! #     fn read_config(&self, _offset: u64, _data: &mut [u8]) {}
//!     fn write_config(&mut self, _offset: u64, _data: &[u8]) {}
//! # }
//! # struct DummyMem;
//! # impl GuestMemAccess for DummyMem {
//! #     fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
//! #     fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
//! # }
//! // 1. Instantiate your backend device and memory.
//! let backend = DummyBackend;
//! let mem = Arc::new(DummyMem);
//!
//! // 2. Wrap the backend in the transport layer and give it an IRQ number.
//! let transport = VirtioMmioTransport::new(backend, mem, 5);
//! ```

use std::sync::Arc;

use hitz_hal::GuestMemAccess;

use crate::mmio_bus::MmioDevice;
use crate::virtio::queue::VirtQueue;

// -- MMIO register offsets ----------------------------------------------------

/// Magic value register -- always `0x7472_6976` ("virt").
const MMIO_MAGIC: u64 = 0x000;
/// Version register -- always 2 (virtio-mmio v2).
const MMIO_VERSION: u64 = 0x004;
/// Device ID register -- identifies the device type.
const MMIO_DEVICE_ID: u64 = 0x008;
/// Vendor ID register.
const MMIO_VENDOR_ID: u64 = 0x00C;
/// Device features (selected page).
const MMIO_DEVICE_FEATURES: u64 = 0x010;
/// Device features page selector (write-only).
const MMIO_DEVICE_FEATURES_SEL: u64 = 0x014;
/// Driver-accepted features (write-only).
const MMIO_DRIVER_FEATURES: u64 = 0x020;
/// Driver features page selector (write-only).
const MMIO_DRIVER_FEATURES_SEL: u64 = 0x024;
/// Queue selector (write-only).
const MMIO_QUEUE_SEL: u64 = 0x030;
/// Maximum queue size (read-only).
const MMIO_QUEUE_NUM_MAX: u64 = 0x034;
/// Actual queue size (write-only).
const MMIO_QUEUE_NUM: u64 = 0x038;
/// Queue ready flag (read-write).
const MMIO_QUEUE_READY: u64 = 0x044;
/// Queue notify (write-only) -- triggers queue processing.
const MMIO_QUEUE_NOTIFY: u64 = 0x050;
/// Interrupt status (read-only).
const MMIO_INTERRUPT_STATUS: u64 = 0x060;
/// Interrupt acknowledge (write-only).
const MMIO_INTERRUPT_ACK: u64 = 0x064;
/// Device status (read-write).
const MMIO_STATUS: u64 = 0x070;
/// Queue descriptor table GPA, low 32 bits.
const MMIO_QUEUE_DESC_LOW: u64 = 0x080;
/// Queue descriptor table GPA, high 32 bits.
const MMIO_QUEUE_DESC_HIGH: u64 = 0x084;
/// Queue available ring GPA, low 32 bits.
const MMIO_QUEUE_AVAIL_LOW: u64 = 0x090;
/// Queue available ring GPA, high 32 bits.
const MMIO_QUEUE_AVAIL_HIGH: u64 = 0x094;
/// Queue used ring GPA, low 32 bits.
const MMIO_QUEUE_USED_LOW: u64 = 0x0A0;
/// Queue used ring GPA, high 32 bits.
const MMIO_QUEUE_USED_HIGH: u64 = 0x0A4;
/// Config generation counter.
const MMIO_CONFIG_GENERATION: u64 = 0x0FC;
/// Start of device-specific config space.
const MMIO_CONFIG_START: u64 = 0x100;

// -- Well-known constants -----------------------------------------------------

/// The magic value identifying a virtio-MMIO device.
const VIRTIO_MMIO_MAGIC: u32 = 0x7472_6976;
/// We implement virtio-MMIO version 2.
const VIRTIO_MMIO_VERSION: u32 = 2;
/// Vendor ID ("QEMU" in little-endian ASCII).
const VIRTIO_VENDOR_ID: u32 = 0x554D_4551;
/// Maximum queue size we support.
const QUEUE_NUM_MAX: u16 = 256;

// -- Status bits --------------------------------------------------------------

/// Guest OS has found the device.
pub const STATUS_ACKNOWLEDGE: u8 = 0x01;
/// Guest OS knows how to drive the device.
pub const STATUS_DRIVER: u8 = 0x02;
/// Feature negotiation complete.
pub const STATUS_FEATURES_OK: u8 = 0x08;
/// Driver setup complete, device is live.
pub const STATUS_DRIVER_OK: u8 = 0x04;
/// Something went wrong in the guest.
pub const STATUS_FAILED: u8 = 0x80;

/// A virtio device backend that processes queue requests.
///
/// Implementations provide the device-specific logic (block, net, etc.)
/// while the [`VirtioMmioTransport`] handles MMIO register decode and
/// queue management.
pub trait VirtioBackend: Send {
    /// Return the virtio device ID (e.g., 2 for block, 1 for net).
    fn device_id(&self) -> u32;

    /// Return the 64-bit device feature bits.
    fn device_features(&self) -> u64;

    /// Number of virtqueues this device uses.
    ///
    /// Defaults to 1. Devices with multiple queues (e.g. net with RX + TX)
    /// should override this.
    fn queue_count(&self) -> usize {
        1
    }

    /// Process all pending requests on the given queue.
    ///
    /// `queue_idx` identifies which virtqueue is being processed (the value
    /// the guest wrote to `QUEUE_NOTIFY`).
    fn process_queue(&mut self, queue_idx: u16, queue: &mut VirtQueue, mem: &dyn GuestMemAccess);

    /// Poll for asynchronous received data (e.g. network RX).
    ///
    /// Called on every run-loop iteration. The implementation should check
    /// for pending data and, if available, write it into the RX queue
    /// (conventionally queue 0). Returns `true` if data was written and
    /// an interrupt should be raised.
    fn poll_rx(&mut self, _rx_queue: &mut VirtQueue, _mem: &dyn GuestMemAccess) -> bool {
        false
    }

    /// Read from device-specific config space.
    ///
    /// `offset` is relative to the start of config space (MMIO 0x100).
    fn read_config(&self, offset: u64, data: &mut [u8]);

    /// Write to device-specific config space.
    ///
    /// `offset` is relative to the start of config space (MMIO 0x100).
    fn write_config(&mut self, offset: u64, data: &[u8]);
}

/// Per-queue state tracked by the MMIO transport.
///
/// Each virtqueue has its own ring GPAs, size, and ready flag, all
/// configured independently by the guest via `QUEUE_SEL` + register writes.
struct QueueState {
    /// The virtqueue itself.
    queue: VirtQueue,
    /// Queue descriptor table GPA, low 32 bits.
    desc_low: u32,
    /// Queue descriptor table GPA, high 32 bits.
    desc_high: u32,
    /// Queue available ring GPA, low 32 bits.
    avail_low: u32,
    /// Queue available ring GPA, high 32 bits.
    avail_high: u32,
    /// Queue used ring GPA, low 32 bits.
    used_low: u32,
    /// Queue used ring GPA, high 32 bits.
    used_high: u32,
    /// Actual queue size set by the guest.
    num: u16,
}

/// Virtio MMIO transport wrapping a backend device.
///
/// Implements `MmioDevice` and manages the MMIO register file,
/// virtqueue configuration, and device status state machine. Supports
/// multiple virtqueues as reported by `VirtioBackend::queue_count`.
pub struct VirtioMmioTransport<D: VirtioBackend> {
    /// The backend device.
    device: D,
    /// Per-queue state (one entry per virtqueue).
    queues: Vec<QueueState>,
    /// Currently selected queue index (via `QUEUE_SEL` register).
    queue_sel: usize,
    /// Reference to guest memory for queue operations.
    mem: Arc<dyn GuestMemAccess>,
    /// Device status register (guest-driven state machine).
    status: u8,
    /// Pending interrupt status bits.
    interrupt_status: u32,
    /// IRQ vector to return when raising an interrupt.
    irq_vector: u8,
    /// Which feature page the guest is reading.
    device_features_sel: u32,
    /// Feature bits accepted by the driver.
    driver_features: u64,
    /// Which feature page the driver is writing.
    driver_features_sel: u32,
    /// Config space generation counter (bumped on config change).
    config_generation: u32,
}

impl<D: VirtioBackend> VirtioMmioTransport<D> {
    /// Create a new MMIO transport for the given backend.
    ///
    /// Allocates one internal queue state per queue reported by
    /// `VirtioBackend::queue_count`.
    ///
    /// * `device` -- the virtio backend (block, net, etc.)
    /// * `mem` -- shared reference to guest memory
    /// * `irq_vector` -- interrupt vector number for this device
    #[must_use]
    pub fn new(device: D, mem: Arc<dyn GuestMemAccess>, irq_vector: u8) -> Self {
        let queue_count = device.queue_count();
        let queues = (0..queue_count)
            .map(|_| QueueState {
                queue: VirtQueue::new(QUEUE_NUM_MAX),
                desc_low: 0,
                desc_high: 0,
                avail_low: 0,
                avail_high: 0,
                used_low: 0,
                used_high: 0,
                num: QUEUE_NUM_MAX,
            })
            .collect();
        Self {
            device,
            queues,
            queue_sel: 0,
            mem,
            status: 0,
            interrupt_status: 0,
            irq_vector,
            device_features_sel: 0,
            driver_features: 0,
            driver_features_sel: 0,
            config_generation: 0,
        }
    }

    /// Reset the transport and device to initial state.
    fn reset(&mut self) {
        self.status = 0;
        self.interrupt_status = 0;
        self.driver_features = 0;
        self.driver_features_sel = 0;
        self.device_features_sel = 0;
        self.queue_sel = 0;
        for qs in &mut self.queues {
            qs.queue.reset();
            qs.desc_low = 0;
            qs.desc_high = 0;
            qs.avail_low = 0;
            qs.avail_high = 0;
            qs.used_low = 0;
            qs.used_high = 0;
            qs.num = QUEUE_NUM_MAX;
        }
    }

    /// Read a 32-bit MMIO register.
    fn read_reg(&self, offset: u64) -> u32 {
        match offset {
            MMIO_MAGIC => VIRTIO_MMIO_MAGIC,
            MMIO_VERSION => VIRTIO_MMIO_VERSION,
            MMIO_DEVICE_ID => self.device.device_id(),
            MMIO_VENDOR_ID => VIRTIO_VENDOR_ID,
            MMIO_DEVICE_FEATURES => {
                let features = self.device.device_features();
                if self.device_features_sel == 0 {
                    // Low 32 bits.
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        features as u32
                    }
                } else {
                    // High 32 bits.
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        (features >> 32) as u32
                    }
                }
            }
            MMIO_QUEUE_NUM_MAX => u32::from(QUEUE_NUM_MAX),
            MMIO_QUEUE_READY => self
                .queues
                .get(self.queue_sel)
                .map_or(0, |qs| u32::from(qs.queue.is_ready())),
            MMIO_INTERRUPT_STATUS => self.interrupt_status,
            MMIO_STATUS => u32::from(self.status),
            MMIO_CONFIG_GENERATION => self.config_generation,
            _ => {
                tracing::debug!(offset, "unhandled MMIO read");
                0
            }
        }
    }

    /// Write a 32-bit MMIO register. Returns `Some(irq)` if an interrupt should fire.
    #[allow(clippy::too_many_lines)]
    fn write_reg(&mut self, offset: u64, value: u32) -> Option<u8> {
        match offset {
            MMIO_DEVICE_FEATURES_SEL => {
                self.device_features_sel = value;
            }
            MMIO_DRIVER_FEATURES => {
                if self.driver_features_sel == 0 {
                    // Low 32 bits.
                    self.driver_features =
                        (self.driver_features & 0xFFFF_FFFF_0000_0000) | u64::from(value);
                } else {
                    // High 32 bits.
                    self.driver_features =
                        (self.driver_features & 0x0000_0000_FFFF_FFFF) | (u64::from(value) << 32);
                }
            }
            MMIO_DRIVER_FEATURES_SEL => {
                self.driver_features_sel = value;
            }
            MMIO_QUEUE_SEL => {
                #[allow(clippy::cast_possible_truncation)]
                let sel = value as usize;
                if sel < self.queues.len() {
                    self.queue_sel = sel;
                } else {
                    tracing::debug!(value, "guest selected non-existent queue");
                }
            }
            MMIO_QUEUE_NUM => {
                // Queue size fits in u16 (max 256). Higher bits are ignored per spec.
                #[allow(clippy::cast_possible_truncation)]
                let size = value as u16;
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.num = size;
                    qs.queue.set_size(size);
                }
            }
            MMIO_QUEUE_READY => {
                let ready = value != 0;
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    if ready {
                        // Configure the queue GPAs before marking ready.
                        let desc_gpa = u64::from(qs.desc_low) | (u64::from(qs.desc_high) << 32);
                        let avail_gpa = u64::from(qs.avail_low) | (u64::from(qs.avail_high) << 32);
                        let used_gpa = u64::from(qs.used_low) | (u64::from(qs.used_high) << 32);
                        qs.queue.configure(desc_gpa, avail_gpa, used_gpa);
                    }
                    qs.queue.set_ready(ready);
                }
            }
            MMIO_QUEUE_NOTIFY => {
                // Value is the queue index the guest is notifying.
                #[allow(clippy::cast_possible_truncation)]
                let queue_idx = value as u16;
                if let Some(qs) = self.queues.get_mut(usize::from(queue_idx)) {
                    self.device
                        .process_queue(queue_idx, &mut qs.queue, &*self.mem);
                    // Signal used ring update.
                    self.interrupt_status |= 1;
                    return Some(self.irq_vector);
                }
            }
            MMIO_INTERRUPT_ACK => {
                self.interrupt_status &= !value;
            }
            MMIO_STATUS => {
                // Status is an 8-bit register. Higher bits are ignored per spec.
                #[allow(clippy::cast_possible_truncation)]
                let val = value as u8;
                if val == 0 {
                    // Writing 0 resets the device.
                    self.reset();
                } else {
                    self.status = val;
                }
            }
            MMIO_QUEUE_DESC_LOW => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.desc_low = value;
                }
            }
            MMIO_QUEUE_DESC_HIGH => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.desc_high = value;
                }
            }
            MMIO_QUEUE_AVAIL_LOW => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.avail_low = value;
                }
            }
            MMIO_QUEUE_AVAIL_HIGH => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.avail_high = value;
                }
            }
            MMIO_QUEUE_USED_LOW => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.used_low = value;
                }
            }
            MMIO_QUEUE_USED_HIGH => {
                if let Some(qs) = self.queues.get_mut(self.queue_sel) {
                    qs.used_high = value;
                }
            }
            _ => {
                tracing::debug!(offset, value, "unhandled MMIO write");
            }
        }
        None
    }
}

impl<D: VirtioBackend> MmioDevice for VirtioMmioTransport<D> {
    fn mmio_read(&mut self, offset: u64, data: &mut [u8]) {
        if offset >= MMIO_CONFIG_START {
            // Device-specific config space.
            self.device.read_config(offset - MMIO_CONFIG_START, data);
            return;
        }

        let value = self.read_reg(offset);
        let bytes = value.to_le_bytes();
        let len = data.len().min(bytes.len());
        data[..len].copy_from_slice(&bytes[..len]);
    }

    fn mmio_write(&mut self, offset: u64, data: &[u8], _mem: &dyn GuestMemAccess) -> Option<u8> {
        if offset >= MMIO_CONFIG_START {
            self.device.write_config(offset - MMIO_CONFIG_START, data);
            return None;
        }

        // Parse the value from little-endian bytes.
        let mut buf = [0u8; 4];
        let len = data.len().min(4);
        buf[..len].copy_from_slice(&data[..len]);
        let value = u32::from_le_bytes(buf);

        self.write_reg(offset, value)
    }

    fn poll_rx(&mut self) -> Option<u8> {
        // RX queue is always queue 0 by virtio convention.
        if let Some(qs) = self.queues.first_mut()
            && self.device.poll_rx(&mut qs.queue, &*self.mem)
        {
            self.interrupt_status |= 1;
            return Some(self.irq_vector);
        }
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    unused_results
)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Minimal backend for testing the transport layer.
    struct DummyBackend {
        features: u64,
        config: Vec<u8>,
        process_count: u32,
    }

    impl DummyBackend {
        fn new() -> Self {
            Self {
                features: 0,
                config: vec![0u8; 16],
                process_count: 0,
            }
        }
    }

    impl VirtioBackend for DummyBackend {
        fn device_id(&self) -> u32 {
            42
        }

        fn device_features(&self) -> u64 {
            self.features
        }

        fn process_queue(
            &mut self,
            _queue_idx: u16,
            _queue: &mut VirtQueue,
            _mem: &dyn GuestMemAccess,
        ) {
            self.process_count += 1;
        }

        fn read_config(&self, offset: u64, data: &mut [u8]) {
            let start = offset as usize;
            let end = (start + data.len()).min(self.config.len());
            if start < self.config.len() {
                let len = end - start;
                data[..len].copy_from_slice(&self.config[start..end]);
            }
        }

        fn write_config(&mut self, offset: u64, data: &[u8]) {
            let start = offset as usize;
            let end = (start + data.len()).min(self.config.len());
            if start < self.config.len() {
                let len = end - start;
                self.config[start..end].copy_from_slice(&data[..len]);
            }
        }
    }

    /// Mock guest memory.
    struct MockMem {
        inner: Mutex<Vec<u8>>,
    }

    impl MockMem {
        fn new(size: usize) -> Self {
            Self {
                inner: Mutex::new(vec![0u8; size]),
            }
        }
    }

    impl GuestMemAccess for MockMem {
        fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), hitz_hal::HalError> {
            let mem = self.inner.lock().unwrap();
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
            let mut mem = self.inner.lock().unwrap();
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

    fn make_transport() -> VirtioMmioTransport<DummyBackend> {
        let mem = Arc::new(MockMem::new(0x10000));
        VirtioMmioTransport::new(DummyBackend::new(), mem, 5)
    }

    fn read_u32(transport: &mut VirtioMmioTransport<DummyBackend>, offset: u64) -> u32 {
        let mut buf = [0u8; 4];
        transport.mmio_read(offset, &mut buf);
        u32::from_le_bytes(buf)
    }

    fn write_u32(
        transport: &mut VirtioMmioTransport<DummyBackend>,
        offset: u64,
        value: u32,
    ) -> Option<u8> {
        let mem = Arc::new(MockMem::new(16));
        transport.mmio_write(offset, &value.to_le_bytes(), &*mem)
    }

    #[test]
    fn magic_value() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, MMIO_MAGIC), 0x7472_6976);
    }

    #[test]
    fn version_is_2() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, MMIO_VERSION), 2);
    }

    #[test]
    fn device_id_from_backend() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, MMIO_DEVICE_ID), 42);
    }

    #[test]
    fn vendor_id() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, MMIO_VENDOR_ID), 0x554D_4551);
    }

    #[test]
    fn queue_num_max() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, MMIO_QUEUE_NUM_MAX), 256);
    }

    #[test]
    fn status_state_machine() {
        let mut t = make_transport();

        // Initial status is 0.
        assert_eq!(read_u32(&mut t, MMIO_STATUS), 0);

        // ACKNOWLEDGE.
        write_u32(&mut t, MMIO_STATUS, u32::from(STATUS_ACKNOWLEDGE));
        assert_eq!(read_u32(&mut t, MMIO_STATUS), u32::from(STATUS_ACKNOWLEDGE));

        // DRIVER.
        write_u32(
            &mut t,
            MMIO_STATUS,
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER),
        );
        assert_eq!(
            read_u32(&mut t, MMIO_STATUS),
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER)
        );

        // FEATURES_OK.
        write_u32(
            &mut t,
            MMIO_STATUS,
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK),
        );
        assert_eq!(
            read_u32(&mut t, MMIO_STATUS),
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK)
        );

        // DRIVER_OK.
        write_u32(
            &mut t,
            MMIO_STATUS,
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK),
        );
        assert_eq!(
            read_u32(&mut t, MMIO_STATUS),
            u32::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK)
        );
    }

    #[test]
    fn status_reset() {
        let mut t = make_transport();

        // Set some status.
        write_u32(&mut t, MMIO_STATUS, u32::from(STATUS_ACKNOWLEDGE));
        assert_eq!(read_u32(&mut t, MMIO_STATUS), u32::from(STATUS_ACKNOWLEDGE));

        // Write 0 resets.
        write_u32(&mut t, MMIO_STATUS, 0);
        assert_eq!(read_u32(&mut t, MMIO_STATUS), 0);
    }

    #[test]
    fn device_features_paging() {
        let mem = Arc::new(MockMem::new(0x10000));
        let mut backend = DummyBackend::new();
        backend.features = 0xDEAD_BEEF_CAFE_BABE;
        let mut t = VirtioMmioTransport::new(backend, mem, 5);

        // Page 0 = low 32 bits.
        write_u32(&mut t, MMIO_DEVICE_FEATURES_SEL, 0);
        assert_eq!(read_u32(&mut t, MMIO_DEVICE_FEATURES), 0xCAFE_BABE);

        // Page 1 = high 32 bits.
        write_u32(&mut t, MMIO_DEVICE_FEATURES_SEL, 1);
        assert_eq!(read_u32(&mut t, MMIO_DEVICE_FEATURES), 0xDEAD_BEEF);
    }

    #[test]
    fn queue_notify_triggers_processing() {
        let mut t = make_transport();

        // Notify should trigger process_queue and return IRQ.
        let irq = write_u32(&mut t, MMIO_QUEUE_NOTIFY, 0);
        assert_eq!(irq, Some(5));

        // Interrupt status should have bit 0 set.
        assert_eq!(read_u32(&mut t, MMIO_INTERRUPT_STATUS), 1);

        // ACK the interrupt.
        write_u32(&mut t, MMIO_INTERRUPT_ACK, 1);
        assert_eq!(read_u32(&mut t, MMIO_INTERRUPT_STATUS), 0);
    }

    #[test]
    fn config_space_read_write() {
        let mut t = make_transport();

        // Write to config space at offset 0x100 + 4.
        let fake_mem = MockMem::new(16);
        let data = 0xDEAD_BEEFu32.to_le_bytes();
        t.mmio_write(MMIO_CONFIG_START + 4, &data, &fake_mem);

        // Read back.
        let mut buf = [0u8; 4];
        t.mmio_read(MMIO_CONFIG_START + 4, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 0xDEAD_BEEF);
    }

    #[test]
    fn queue_ready_configures_gpas() {
        let mut t = make_transport();

        // Set the GPA registers.
        write_u32(&mut t, MMIO_QUEUE_DESC_LOW, 0x1000);
        write_u32(&mut t, MMIO_QUEUE_DESC_HIGH, 0);
        write_u32(&mut t, MMIO_QUEUE_AVAIL_LOW, 0x2000);
        write_u32(&mut t, MMIO_QUEUE_AVAIL_HIGH, 0);
        write_u32(&mut t, MMIO_QUEUE_USED_LOW, 0x3000);
        write_u32(&mut t, MMIO_QUEUE_USED_HIGH, 0);

        // Mark ready.
        write_u32(&mut t, MMIO_QUEUE_READY, 1);
        assert_eq!(read_u32(&mut t, MMIO_QUEUE_READY), 1);
        assert!(t.queues[0].queue.is_ready());
    }

    #[test]
    fn queue_sel_out_of_bounds() {
        struct OneQueueBackend;
        impl VirtioBackend for OneQueueBackend {
            fn device_id(&self) -> u32 {
                42
            }
            fn device_features(&self) -> u64 {
                0
            }
            fn process_queue(&mut self, _idx: u16, _q: &mut VirtQueue, _m: &dyn GuestMemAccess) {}
            fn read_config(&self, _off: u64, _data: &mut [u8]) {}
            fn write_config(&mut self, _off: u64, _data: &[u8]) {}
            fn queue_count(&self) -> usize {
                1
            }
        }
        let mem = Arc::new(MockMem::new(0x10000));
        let mut t = VirtioMmioTransport::new(OneQueueBackend, mem.clone(), 5);

        // Try to select queue 1 (out of bounds).
        let _ = t.mmio_write(MMIO_QUEUE_SEL, &1u32.to_le_bytes(), &*mem);
        // Should not change queue_sel from 0.
        assert_eq!(t.queue_sel, 0);
    }

    #[test]
    fn write_unmapped_returns_none() {
        let mut t = make_transport();
        assert_eq!(write_u32(&mut t, 0xFFFF, 0), None);
    }

    #[test]
    fn read_unmapped_returns_0() {
        let mut t = make_transport();
        assert_eq!(read_u32(&mut t, 0xFFFF), 0);
    }

    #[test]
    fn write_queue_num_modifies_selected_queue() {
        let mut t = make_transport();

        // Write to valid queue 0
        write_u32(&mut t, MMIO_QUEUE_SEL, 0);
        write_u32(&mut t, MMIO_QUEUE_NUM, 16);
        assert_eq!(t.queues[0].num, 16);
    }

    #[test]
    fn write_driver_features_paging() {
        let mut t = make_transport();

        // Write low 32 bits
        write_u32(&mut t, MMIO_DRIVER_FEATURES_SEL, 0);
        write_u32(&mut t, MMIO_DRIVER_FEATURES, 0x1234_5678);
        assert_eq!(t.driver_features, 0x1234_5678);

        // Write high 32 bits
        write_u32(&mut t, MMIO_DRIVER_FEATURES_SEL, 1);
        write_u32(&mut t, MMIO_DRIVER_FEATURES, 0x9ABC_DEF0);
        assert_eq!(t.driver_features, 0x9ABC_DEF0_1234_5678);

        // Re-write low 32 bits
        write_u32(&mut t, MMIO_DRIVER_FEATURES_SEL, 0);
        write_u32(&mut t, MMIO_DRIVER_FEATURES, 0x0000_0000);
        assert_eq!(t.driver_features, 0x9ABC_DEF0_0000_0000);
    }

    #[test]
    fn write_queue_address_registers() {
        let mut t = make_transport();

        // Write to valid queue 0
        write_u32(&mut t, MMIO_QUEUE_SEL, 0);

        write_u32(&mut t, MMIO_QUEUE_DESC_LOW, 0x1111);
        write_u32(&mut t, MMIO_QUEUE_DESC_HIGH, 0x2222);

        write_u32(&mut t, MMIO_QUEUE_AVAIL_LOW, 0x3333);
        write_u32(&mut t, MMIO_QUEUE_AVAIL_HIGH, 0x4444);

        write_u32(&mut t, MMIO_QUEUE_USED_LOW, 0x5555);
        write_u32(&mut t, MMIO_QUEUE_USED_HIGH, 0x6666);

        assert_eq!(t.queues[0].desc_low, 0x1111);
        assert_eq!(t.queues[0].desc_high, 0x2222);
        assert_eq!(t.queues[0].avail_low, 0x3333);
        assert_eq!(t.queues[0].avail_high, 0x4444);
        assert_eq!(t.queues[0].used_low, 0x5555);
        assert_eq!(t.queues[0].used_high, 0x6666);
    }

    #[test]
    fn write_config_generation_does_nothing() {
        let mut t = make_transport();
        assert_eq!(t.config_generation, 0);
        write_u32(&mut t, MMIO_CONFIG_GENERATION, 1);
        assert_eq!(t.config_generation, 0);
    }

    #[test]
    fn poll_rx_returns_none_if_no_queues_or_false() {
        let mut t = make_transport();
        assert_eq!(t.poll_rx(), None);
    }

    #[test]
    fn poll_rx_returns_irq_if_backend_poll_rx_true() {
        struct PollRxBackend;
        impl VirtioBackend for PollRxBackend {
            fn device_id(&self) -> u32 {
                42
            }
            fn device_features(&self) -> u64 {
                0
            }
            fn process_queue(&mut self, _idx: u16, _q: &mut VirtQueue, _m: &dyn GuestMemAccess) {}
            fn read_config(&self, _off: u64, _data: &mut [u8]) {}
            fn write_config(&mut self, _off: u64, _data: &[u8]) {}
            fn queue_count(&self) -> usize {
                1
            }
            fn poll_rx(&mut self, _rx_queue: &mut VirtQueue, _mem: &dyn GuestMemAccess) -> bool {
                true
            }
        }

        let mem = Arc::new(MockMem::new(0x10000));
        let mut t = VirtioMmioTransport::new(PollRxBackend, mem, 5);

        assert_eq!(t.poll_rx(), Some(5));
        assert_eq!(t.interrupt_status, 1);
    }

    #[test]
    fn write_config_space_unmapped() {
        let mut t = make_transport();
        let fake_mem = MockMem::new(16);
        let data = 0xDEAD_BEEFu32.to_le_bytes();

        // Write exactly at the end of config space.
        // Actually, DummyBackend config is 16 bytes.
        // Write beyond 16 bytes config space.
        t.mmio_write(MMIO_CONFIG_START + 16, &data, &fake_mem);

        let mut buf = [0u8; 4];
        t.mmio_read(MMIO_CONFIG_START + 16, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 0); // Out of bounds read returns 0 (no copy)
    }

    #[test]
    fn write_unaligned_length_returns_none() {
        let mut t = make_transport();
        let mem = Arc::new(MockMem::new(16));

        // Write only 2 bytes (unaligned length). Should pad and process.
        let data: [u8; 2] = [0x11, 0x22];
        assert_eq!(t.mmio_write(MMIO_MAGIC, &data, &*mem), None);
    }

    #[test]
    fn mmio_read_short_data() {
        let mut t = make_transport();

        let mut buf = [0u8; 2];
        t.mmio_read(MMIO_MAGIC, &mut buf);
        // Magic is 0x74726976 (LE: 76 69 72 74)
        assert_eq!(buf, [0x76, 0x69]);
    }

    #[test]
    fn unhandled_mmio_read_returns_zero() {
        let t = make_transport();
        assert_eq!(t.read_reg(0x9999), 0);
    }

    #[test]
    fn unhandled_mmio_write_returns_none() {
        let mut t = make_transport();
        assert_eq!(t.write_reg(0x9999, 0), None);
    }

    #[test]
    fn unhandled_mmio_write_unaligned() {
        let mut t = make_transport();
        let mem = Arc::new(MockMem::new(16));

        // Write unaligned config size
        t.mmio_write(MMIO_CONFIG_START + 0, &[0x11, 0x22], &*mem);

        let mut buf = [0u8; 4];
        t.mmio_read(MMIO_CONFIG_START + 0, &mut buf);
        // Only 2 bytes are written (from dummy config write which copies min len)
        assert_eq!(buf[0], 0x11);
        assert_eq!(buf[1], 0x22);
    }
}
