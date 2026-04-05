#![allow(clippy::cast_possible_truncation)]
#![allow(dead_code)]
//! Device emulation for Hitz.
//!
//! # Abstract
//!
//! This module forms the backbone of hardware emulation for the Hitz micro-VM. It provides
//! the virtual devices necessary for a functional Linux guest.
//!
//! By simulating Memory-Mapped I/O (MMIO) and traditional x86 I/O ports through the
//! [`MmioBus`], it allows the hypervisor to intercept guest device accesses and route them
//! to the appropriate rust-based device handlers.
//!
//! Currently supported devices include a 16550A-compatible serial console ([`SerialDevice`]),
//! a virtio-block storage backend ([`VirtioBlockDevice`]), and a virtio-net network interface ([`VirtioNetDevice`]).
//!
//! # The Hero's Journey
//!
//! Devices are instantiated during the VM setup phase and registered with the central `MmioBus`.
//! Here is a simplified illustration of how devices are attached to the bus:
//!
//! ```no_run
//! use hitz_devices::{MmioBus, SerialDevice, VirtioBlockDevice, VirtioMmioTransport};
//! use std::sync::{Arc, Mutex};
//!
//! // 1. Create the central bus
//! let mut bus = MmioBus::new();
//!
//! // 2. Create devices
//! let mut serial = SerialDevice::new(std::io::sink());
//! let block_backend = VirtioBlockDevice::new(std::fs::File::open("/dev/null").unwrap()).unwrap();
//! let mut block_device = VirtioMmioTransport::new(block_backend, std::sync::Arc::new(DummyMem), 5);
//!

//! // 3. Register devices with specific base addresses and sizes
//! // Typical serial base is 0x3f8 with size 8
//! # struct DummyMem;
//! # impl hitz_hal::GuestMemAccess for DummyMem {
//! #     fn read_guest(&self, _gpa: u64, _buf: &mut [u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
//! #     fn write_guest(&self, _gpa: u64, _data: &[u8]) -> Result<(), hitz_hal::HalError> { Ok(()) }
//! # }
//!
//! // Typical virtio MMIO base is 0xd0000000 with size 0x200
//! bus.register(0xd0000000, 0x200, Box::new(block_device));
//!
//! // 4. When a guest access occurs (intercepted by WHP), route it:
//! // e.g., guest writes 'A' (0x41) to the serial port
//! bus.write(0x3f8, &[0x41], &DummyMem);
//! ```
//!
//! # The Fine Print
//!
//! - **Virtio Over MMIO**: Hitz prefers the virtio-MMIO transport over PCI for simplicity
//!   and reduced surface area. The kernel must be compiled with `CONFIG_VIRTIO_MMIO`
//!   enabled, and device memory regions must be passed to the kernel via command line
//!   (e.g., `virtio_mmio.device=1K@0xd0000000:5`).
//! - **Concurrency**: The `MmioBus` and underlying devices are designed to be shared
//!   across vCPU threads. Mutexes are used extensively, but long-held locks could cause
//!   vCPU stalls.
//!
//! ## Panics
//!
//! Registering a device into the [`MmioBus`] with an overlapping address range will cause a panic
//! to prevent silent memory corruption and unpredictable device behavior.

pub(crate) mod mmio_bus;
pub(crate) mod serial;
pub(crate) mod virtio;

pub use mmio_bus::MmioBus;
pub use serial::SerialDevice;
pub use virtio::VirtQueue;
pub use virtio::VirtioBackend;
pub use virtio::VirtioBlockDevice;
pub use virtio::VirtioMmioTransport;
pub use virtio::VirtioNetDevice;

pub use virtio::{VSOCK_BUF_ALLOC, VirtioVsockDevice, VsockHdr, VsockOp, VsockPacket};
