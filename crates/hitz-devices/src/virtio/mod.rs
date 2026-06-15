//! Virtio device stack: queue, MMIO transport, and backend devices.
//!
//! # Abstract
//!
//! This module contains the implementation of the virtio device models.
//! It provides the abstract [`VirtQueue`] and [`VirtioMmioTransport`] that bridge
//! the gap between guest memory operations and specific virtio device backends like
//! network, block storage, and vsock.
//!
//! # The Hero's Journey
//!
//! ```rust
//! // Virtio devices are created and then attached to the MMIO bus.
//! // For example, a block device:
//! use hitz_devices::{VirtioBlockDevice, VirtioMmioTransport};
//! use std::fs::File;
//!
//! // Assuming we have a valid disk file:
//! // let file = File::open("disk.img").unwrap();
//! // let block_dev = VirtioBlockDevice::new(file);
//! // let transport = VirtioMmioTransport::new(block_dev);
//! //
//! // Now `transport` implements `MmioDevice` and can be registered on the bus!
//! ```

mod block;
mod mmio_transport;
mod net;
pub mod queue;
mod vsock;

pub use block::VirtioBlockDevice;
pub use mmio_transport::VirtioBackend;
pub use mmio_transport::VirtioMmioTransport;
pub use queue::VirtQueue;

pub use net::VirtioNetDevice;
pub use vsock::{VSOCK_BUF_ALLOC, VirtioVsockDevice, VsockHdr, VsockOp, VsockPacket};
