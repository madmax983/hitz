//! Virtio device stack: queue, MMIO transport, and backend devices.

mod block;
mod mmio_transport;
mod net;
mod queue;
mod vsock;

pub use block::VirtioBlockDevice;
pub use mmio_transport::VirtioBackend;
pub use mmio_transport::VirtioMmioTransport;
pub use queue::VirtQueue;

pub use net::VirtioNetDevice;
pub use vsock::{VSOCK_BUF_ALLOC, VirtioVsockDevice, VsockHdr, VsockOp, VsockPacket};
