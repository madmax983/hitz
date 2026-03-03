//! Networking for Hitz.
//!
//! Ethernet frame parsing, ARP handling, virtio-net support,
//! and `WinTun` adapter integration.

pub mod ethernet;
pub mod wintun_io;

pub use wintun_io::{NetIoError, NetIoHandle, start_net_io};
