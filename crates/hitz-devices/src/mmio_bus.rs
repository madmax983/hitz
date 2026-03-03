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
    pub fn register(&mut self, base_gpa: u64, size: u64, device: Box<dyn MmioDevice>) {
        self.slots.push(MmioSlot {
            base: base_gpa,
            size,
            device,
        });
    }

    /// Dispatch a guest read to the device owning `gpa`.
    ///
    /// If no device is registered at `gpa`, `data` is filled with `0xFF`.
    pub fn read(&mut self, gpa: u64, data: &mut [u8]) {
        for slot in &mut self.slots {
            if gpa >= slot.base && gpa < slot.base + slot.size {
                let offset = gpa - slot.base;
                slot.device.mmio_read(offset, data);
                return;
            }
        }
        // No device — return all-ones (standard "nothing here" response).
        data.fill(0xFF);
    }

    /// Dispatch a guest write to the device owning `gpa`.
    ///
    /// Returns `Some(vector)` if the device wants to raise an interrupt.
    pub fn write(&mut self, gpa: u64, data: &[u8], mem: &dyn GuestMemAccess) -> Option<u8> {
        for slot in &mut self.slots {
            if gpa >= slot.base && gpa < slot.base + slot.size {
                let offset = gpa - slot.base;
                return slot.device.mmio_write(offset, data, mem);
            }
        }
        None
    }

    /// Poll all devices for pending asynchronous I/O.
    ///
    /// Returns the first interrupt vector from a device that has data ready,
    /// or `None` if no device needs attention.
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
    }

    impl StubDevice {
        fn new(read_value: u32) -> Self {
            Self {
                last_write_offset: 0,
                last_write_data: Vec::new(),
                read_value,
                irq_on_write: None,
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
    }

    /// Stub GuestMemAccess for testing.
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
}
