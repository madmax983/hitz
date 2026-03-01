//! Strongly-typed newtypes for domain values.
//!
//! Raw primitives invite bugs (swapping a GPA for a size, an IRQ for a vCPU ID).
//! Newtypes make those mistakes compile-time errors.

use std::fmt;

use uuid::Uuid;

/// Unique identifier for a virtual machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmId(Uuid);

impl VmId {
    /// Create a new random VM identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for VmId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Virtual CPU identifier within a partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VcpuId(u32);

impl VcpuId {
    /// Create a new vCPU identifier.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw vCPU index.
    #[must_use]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for VcpuId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vcpu-{}", self.0)
    }
}

/// Guest Physical Address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Gpa(u64);

impl Gpa {
    /// Create a new guest physical address.
    #[must_use]
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address value.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Add an offset to this address.
    #[must_use]
    pub const fn offset(&self, bytes: u64) -> Self {
        Self(self.0 + bytes)
    }

    /// Calculate the page-aligned base (4 KiB pages).
    #[must_use]
    pub const fn page_base(&self) -> Self {
        Self(self.0 & !0xFFF)
    }

    /// Get the offset within a 4 KiB page.
    #[must_use]
    pub const fn page_offset(&self) -> u64 {
        self.0 & 0xFFF
    }
}

impl fmt::Display for Gpa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}", self.0)
    }
}

/// Interrupt request line number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IrqLine(u8);

impl IrqLine {
    /// Create a new IRQ line.
    #[must_use]
    pub const fn new(irq: u8) -> Self {
        Self(irq)
    }

    /// Get the raw IRQ number.
    #[must_use]
    pub const fn as_u8(&self) -> u8 {
        self.0
    }
}

impl fmt::Display for IrqLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IRQ{}", self.0)
    }
}

/// Slot index for MMIO device registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MmioSlot(u16);

impl MmioSlot {
    /// Create a new MMIO slot.
    #[must_use]
    pub const fn new(slot: u16) -> Self {
        Self(slot)
    }

    /// Get the raw slot index.
    #[must_use]
    pub const fn as_u16(&self) -> u16 {
        self.0
    }
}

/// Offset within a disk image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiskOffset(u64);

impl DiskOffset {
    /// Create a new disk offset.
    #[must_use]
    pub const fn new(offset: u64) -> Self {
        Self(offset)
    }

    /// Get the raw byte offset.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// MAC address for a virtual network device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    /// Create a MAC address from raw bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// Memory size in mebibytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemSizeMiB(u64);

impl MemSizeMiB {
    /// Create a new memory size.
    #[must_use]
    pub const fn new(mib: u64) -> Self {
        Self(mib)
    }

    /// Get the size in MiB.
    #[must_use]
    pub const fn as_mib(&self) -> u64 {
        self.0
    }

    /// Convert to bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> u64 {
        self.0 * 1024 * 1024
    }
}

impl fmt::Display for MemSizeMiB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} MiB", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpa_page_alignment() {
        let addr = Gpa::new(0x1234_5678);
        assert_eq!(addr.page_base(), Gpa::new(0x1234_5000));
        assert_eq!(addr.page_offset(), 0x678);
    }

    #[test]
    fn gpa_offset() {
        let base = Gpa::new(0x1000);
        assert_eq!(base.offset(0x500), Gpa::new(0x1500));
    }

    #[test]
    fn mem_size_conversion() {
        let size = MemSizeMiB::new(128);
        assert_eq!(size.as_bytes(), 128 * 1024 * 1024);
    }

    #[test]
    fn mac_address_display() {
        let mac = MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(mac.to_string(), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn vm_id_uniqueness() {
        let a = VmId::new();
        let b = VmId::new();
        assert_ne!(a, b);
    }
}
