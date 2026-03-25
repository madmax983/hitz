#![allow(clippy::cast_possible_truncation)]
#![allow(dead_code)]
//! Device emulation for Hitz.
//!
//! MMIO/IO port bus, virtio-MMIO transport, serial console, block device.

pub(crate) mod mmio_bus;
pub(crate) mod serial;
pub(crate) mod virtio;

pub use mmio_bus::MmioBus;
pub use serial::SerialDevice;
pub use virtio::block::VirtioBlockDevice;
pub use virtio::mmio_transport::VirtioMmioTransport;
pub use virtio::net::VirtioNetDevice;
pub use virtio::vsock::{VSOCK_BUF_ALLOC, VirtioVsockDevice, VsockHdr, VsockOp, VsockPacket};
