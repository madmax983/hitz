//! MMIO device bus — routes guest MMIO accesses to registered devices.
//!
//! Each device occupies a contiguous region of the guest physical address
//! space. The bus resolves a GPA to the owning device and forwards the
//! access as a device-relative offset.

use hitz_hal::GuestMemAccess;

/// Trait implemented by MMIO-mapped devices (e.g. virtio-MMIO transport).
///
/// All offsets are relative to the device's base GPA.
pub trait MmioDevice: Send {
    /// Handle a guest read from the device.
    fn mmio_read(&mut self, offset: u64, data: &mut [u8]);

    /// Handle a guest write to the device.
    ///
    /// Returns `Some(vector)` if the device wants to raise an interrupt.
    fn mmio_write(&mut self, offset: u64, data: &[u8], mem: &dyn GuestMemAccess) -> Option<u8>;

    /// Poll for asynchronous I/O (e.g. network RX).
    ///
    /// Called on every run-loop iteration. Returns `Some(vector)` if the
    /// device wrote data and needs to raise an interrupt.
    fn poll_rx(&mut self) -> Option<u8> {
        None
    }
}

/// A registered device slot on the MMIO bus.
struct MmioSlot {
    /// Base guest physical address of this device's MMIO region.
    base: u64,
    /// Size of the MMIO region in bytes.
    size: u64,
    /// The device handling accesses to this region.
    device: Box<dyn MmioDevice>,
}

/// Routes MMIO accesses to the correct device based on GPA.
/// A memory-mapped I/O (MMIO) bus that multiplexes reads and writes to virtual devices.
///
/// # Abstract
///
/// `MmioBus` acts as the switchboard for guest memory accesses that fall into the MMIO
/// range (typically above RAM or in a reserved hole). It holds a collection of registered
/// devices (which implement the [`MmioDevice`] trait) and routes `mmio_read` and
/// `mmio_write` calls to the correct device based on the address.
///
/// # The Hero's Journey
///
/// ```
/// # use hitz_devices::{MmioBus, MmioDevice};
/// # use hitz_hal::GuestMemAccess;
/// #
/// # struct DummyDevice;
/// # impl MmioDevice for DummyDevice {
/// #     fn mmio_read(&mut self, _: u64, buf: &mut [u8]) { buf.fill(0x42); }
/// #     fn mmio_write(&mut self, _: u64, _: &[u8], _: &dyn GuestMemAccess) -> Option<u8> { None }
/// # }
/// // 1. Create the MMIO bus.
/// let mut bus = MmioBus::new();
///
/// // 2. Register a device at base address 0x1000 with size 256 bytes.
/// bus.register(0x1000, 256, Box::new(DummyDevice));
///
/// // 3. The guest reads 4 bytes from address 0x1004.
/// let mut buf = [0u8; 4];
/// bus.read(0x1004, &mut buf);
/// assert_eq!(buf, [0x42, 0x42, 0x42, 0x42]);
/// ```
///
/// # The Fine Print
///
/// Devices must not overlap. Attempting to register overlapping address ranges
/// will cause a panic to prevent unpredictable routing and memory corruption.
pub struct MmioBus {
    slots: Vec<MmioSlot>,
}

impl MmioBus {
    /// Create an empty MMIO bus with no devices.
    #[must_use]
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Register a device at `base_gpa` occupying `size` bytes.
    ///
    /// # Panics
    /// Panics if the requested address range overlaps with an already registered device.
    pub fn register(&mut self, base_gpa: u64, size: u64, device: Box<dyn MmioDevice>) {
        for slot in &self.slots {
            let end_gpa = base_gpa + size;
            let slot_end = slot.base + slot.size;
            assert!(
                !(base_gpa < slot_end && end_gpa > slot.base),
                "Overlapping MMIO region: base {base_gpa:#x}, size {size:#x} overlaps with existing device at {:#x}",
                slot.base
            );
        }
        self.slots.push(MmioSlot {
            base: base_gpa,
            size,
            device,
        });

        // ⚡ Bolt Optimization: Keep slots sorted by base address to enable O(log N) binary search
        // during high-frequency MMIO reads/writes on the VM exit path.
        self.slots.sort_by_key(|s| s.base);
    }

    fn find_slot(&mut self, gpa: u64) -> Option<&mut MmioSlot> {
        // ⚡ Bolt Optimization: Replace O(N) linear scan with O(log N) binary search.
        // This significantly reduces VM exit latency when many MMIO devices (like virtio) are registered.
        let idx = self.slots.partition_point(|s| s.base <= gpa);
        if idx > 0 {
            let slot = &mut self.slots[idx - 1];
            if gpa < slot.base + slot.size {
                return Some(slot);
            }
        }
        None
    }

    /// Dispatch a guest read to the device owning `gpa`.
    ///
    /// If no device is registered at `gpa`, `data` is filled with `0xFF`.
    pub fn read(&mut self, gpa: u64, data: &mut [u8]) {
        if let Some(slot) = self.find_slot(gpa) {
            let offset = gpa - slot.base;
            slot.device.mmio_read(offset, data);
            return;
        }
        // No device — return all-ones (standard "nothing here" response).
        data.fill(0xFF);
    }

    /// Dispatch a guest write to the device owning `gpa`.
    ///
    /// Returns `Some(vector)` if the device wants to raise an interrupt.
    pub fn write(&mut self, gpa: u64, data: &[u8], mem: &dyn GuestMemAccess) -> Option<u8> {
        if let Some(slot) = self.find_slot(gpa) {
            let offset = gpa - slot.base;
            return slot.device.mmio_write(offset, data, mem);
        }
        None
    }

    /// Poll all devices for pending asynchronous I/O.
    ///
    /// Scans all registered devices to check if any have pending interrupts to inject.
    ///
    /// # Abstract
    /// Acts as an asynchronous event multiplexer for the MMIO bus. Devices like virtio
    /// block or network adapters raise virtual interrupts when I/O completes. This
    /// function collects those signals so the VMM can inject an IRQ into the guest vCPU.
    ///
    /// # Returns
    /// The first interrupt vector (IRQ number) from a device that requires attention,
    /// or `None` if the bus is quiet.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use hitz_devices::{MmioBus, MmioDevice};
    /// # use hitz_hal::GuestMemAccess;
    /// # struct DummyDevice;
    /// # impl MmioDevice for DummyDevice {
    /// #     fn mmio_read(&mut self, _offset: u64, _data: &mut [u8]) {}
    /// #     fn mmio_write(&mut self, _offset: u64, _data: &[u8], _mem: &dyn GuestMemAccess) -> Option<u8> { None }
    /// #     fn poll_rx(&mut self) -> Option<u8> { Some(5) } // Always demands IRQ 5
    /// # }
    /// let mut bus = MmioBus::new();
    /// bus.register(0xD000_0000, 0x1000, Box::new(DummyDevice));
    ///
    /// if let Some(irq) = bus.poll_devices() {
    ///     assert_eq!(irq, 5);
    ///     // Inject IRQ 5 into the guest...
    /// }
    /// ```
    pub fn poll_devices(&mut self) -> Option<u8> {
        for slot in &mut self.slots {
            if let Some(irq) = slot.device.poll_rx() {
                return Some(irq);
            }
        }
        None
    }
}

impl Default for MmioBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hitz_hal::HalError;

    /// Stub MMIO device for testing.
    struct StubDevice {
        last_write_offset: u64,
        last_write_data: Vec<u8>,
        read_value: u32,
        irq_on_write: Option<u8>,
        irq_on_poll_rx: Option<u8>,
    }

    impl StubDevice {
        fn new(read_value: u32) -> Self {
            Self {
                last_write_offset: 0,
                last_write_data: Vec::new(),
                read_value,
                irq_on_write: None,
                irq_on_poll_rx: None,
            }
        }
    }

    impl MmioDevice for StubDevice {
        fn mmio_read(&mut self, _offset: u64, data: &mut [u8]) {
            let bytes = self.read_value.to_le_bytes();
            let len = data.len().min(bytes.len());
            data[..len].copy_from_slice(&bytes[..len]);
        }

        fn mmio_write(
            &mut self,
            offset: u64,
            data: &[u8],
            _mem: &dyn GuestMemAccess,
        ) -> Option<u8> {
            self.last_write_offset = offset;
            self.last_write_data = data.to_vec();
            self.irq_on_write
        }

        fn poll_rx(&mut self) -> Option<u8> {
            self.irq_on_poll_rx
        }
    }

    /// Stub `GuestMemAccess` for testing.
    struct StubMem;
    impl GuestMemAccess for StubMem {
        fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), HalError> {
            Ok(())
        }
        fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), HalError> {
            Ok(())
        }
    }

    #[test]
    fn read_from_registered_device() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0xDEAD_BEEF)));

        let mut data = [0u8; 4];
        bus.read(0xD000_0000, &mut data);
        assert_eq!(u32::from_le_bytes(data), 0xDEAD_BEEF);
    }

    #[test]
    fn read_with_offset() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0x1234)));

        let mut data = [0u8; 4];
        bus.read(0xD000_0010, &mut data);
        // StubDevice ignores offset, always returns read_value.
        assert_eq!(u32::from_le_bytes(data), 0x1234);
    }

    #[test]
    fn read_unmapped_returns_ff() {
        let mut bus = MmioBus::new();
        let mut data = [0u8; 4];
        bus.read(0xFFFF_0000, &mut data);
        assert_eq!(data, [0xFF; 4]);
    }

    #[test]
    fn write_unmapped_returns_none() {
        let mut bus = MmioBus::new();
        let mem = StubMem;
        let data = [0u8; 4];
        let irq = bus.write(0xFFFF_0000, &data, &mem);
        assert_eq!(irq, None);
    }

    #[test]
    fn write_dispatches_to_device() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));

        let mem = StubMem;
        let data = 0x42u32.to_le_bytes();
        let irq = bus.write(0xD000_0050, &data, &mem);
        assert!(irq.is_none());
    }

    #[test]
    fn write_returns_irq() {
        let mut bus = MmioBus::new();
        let mut dev = StubDevice::new(0);
        dev.irq_on_write = Some(5);
        bus.register(0xD000_0000, 0x1000, Box::new(dev));

        let mem = StubMem;
        let data = [0u8; 4];
        let irq = bus.write(0xD000_0000, &data, &mem);
        assert_eq!(irq, Some(5));
    }

    #[test]
    fn poll_devices_returns_none_when_empty() {
        let mut bus = MmioBus::new();
        assert_eq!(bus.poll_devices(), None);
    }

    #[test]
    fn poll_devices_returns_none_when_no_device_has_data() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
        assert_eq!(bus.poll_devices(), None);
    }

    #[test]
    fn poll_devices_returns_irq() {
        let mut bus = MmioBus::new();
        let mut dev = StubDevice::new(0);
        dev.irq_on_poll_rx = Some(7);
        bus.register(0xD000_0000, 0x1000, Box::new(dev));

        assert_eq!(bus.poll_devices(), Some(7));
    }

    #[test]
    fn poll_devices_returns_first_irq() {
        let mut bus = MmioBus::new();
        let dev1 = StubDevice::new(0); // No IRQ
        let mut dev2 = StubDevice::new(0);
        dev2.irq_on_poll_rx = Some(5);
        let mut dev3 = StubDevice::new(0);
        dev3.irq_on_poll_rx = Some(9);

        bus.register(0xD000_0000, 0x1000, Box::new(dev1));
        bus.register(0xD000_1000, 0x1000, Box::new(dev2));
        bus.register(0xD000_2000, 0x1000, Box::new(dev3));

        // Should return the IRQ from the first device in the slots list
        // that has an IRQ pending (dev2 in this case).
        assert_eq!(bus.poll_devices(), Some(5));
    }

    #[test]
    #[should_panic(
        expected = "Overlapping MMIO region: base 0xd0000500, size 0x1000 overlaps with existing device at 0xd0000000"
    )]
    fn register_overlapping_device_panics() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
        // Overlaps at the end of the existing region
        bus.register(0xD000_0500, 0x1000, Box::new(StubDevice::new(0)));
    }

    #[test]
    #[should_panic(
        expected = "Overlapping MMIO region: base 0xd0000000, size 0x1000 overlaps with existing device at 0xd0000000"
    )]
    fn register_exact_overlap_panics() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
        // Exact overlap
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
    }

    #[test]
    #[should_panic(
        expected = "Overlapping MMIO region: base 0xd0000100, size 0x100 overlaps with existing device at 0xd0000000"
    )]
    fn register_inner_overlap_panics() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
        // Complete inner overlap
        bus.register(0xD000_0100, 0x0100, Box::new(StubDevice::new(0)));
    }

    #[test]
    #[should_panic(
        expected = "Overlapping MMIO region: base 0xcfff0000, size 0x12000 overlaps with existing device at 0xd0000000"
    )]
    fn register_outer_overlap_panics() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_0000, 0x1000, Box::new(StubDevice::new(0)));
        // Complete outer overlap
        bus.register(0xCFFF_0000, 0x12000, Box::new(StubDevice::new(0)));
    }

    #[test]
    fn default_mmio_bus_and_default_device() {
        let _bus = MmioBus::default();
        let mut dev = StubDevice::new(0);
        assert_eq!(MmioDevice::poll_rx(&mut dev), None);
    }
}
