#![allow(dead_code, clippy::trivially_copy_pass_by_ref)]
//! Networking for Hitz.
//!
//! Ethernet frame parsing, ARP handling, virtio-net support,
//! and `WinTun` adapter integration.

pub(crate) mod ethernet;
pub(crate) mod wintun_io;

pub use ethernet::{parse_cidr, parse_mac, random_mac};
pub use wintun_io::{NetIoError, NetIoHandle, start_net_io};
