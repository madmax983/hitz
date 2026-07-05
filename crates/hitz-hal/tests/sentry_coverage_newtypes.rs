#![allow(missing_docs)]
#![allow(clippy::uninlined_format_args)]

use hitz_hal::*;

#[test]
fn test_vmid() {
    let id = VmId::new();
    let uuid = id.as_uuid();
    assert_eq!(*uuid, *id.as_uuid());
    let default_id = VmId::default();
    assert_ne!(id, default_id);
    let display = format!("{}", id);
    assert_eq!(display, id.as_uuid().to_string());
}

#[test]
fn test_vcpuid() {
    let id = VcpuId::new(42);
    assert_eq!(id.as_u32(), 42);
    assert_eq!(format!("{}", id), "vcpu-42");
}

#[test]
fn test_gpa() {
    let gpa = Gpa::new(0x1234_5678);
    assert_eq!(gpa.as_u64(), 0x1234_5678);
    assert_eq!(format!("{}", gpa), format!("0x{:016x}", 0x1234_5678));
}

#[test]
fn test_irqline() {
    let irq = IrqLine::new(7);
    assert_eq!(irq.as_u8(), 7);
    assert_eq!(format!("{}", irq), "IRQ7");
}

#[test]
fn test_mmioslot() {
    let slot = MmioSlot::new(3);
    assert_eq!(slot.as_u16(), 3);
}

#[test]
fn test_diskoffset() {
    let offset = DiskOffset::new(4096);
    assert_eq!(offset.as_u64(), 4096);
}

#[test]
fn test_macaddress() {
    let mac = MacAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    assert_eq!(*mac.as_bytes(), [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
}

#[test]
fn test_memsizemib() {
    let size = MemSizeMiB::new(1024);
    assert_eq!(size.as_mib(), 1024);
    assert_eq!(format!("{}", size), "1024 MiB");
}
