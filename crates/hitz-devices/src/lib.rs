//! Device emulation for Hitz.
//!
//! MMIO/IO port bus, virtio-MMIO transport, serial console, block device.

pub mod mmio_bus;
pub mod serial;
pub mod virtio;

pub use virtio::vsock::VirtioVsockDevice;
