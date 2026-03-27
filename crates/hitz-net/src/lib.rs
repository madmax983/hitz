#![allow(dead_code, clippy::trivially_copy_pass_by_ref)]
//! Networking for Hitz.
//!
//! # Abstract
//!
//! This module encapsulates the host-side networking capabilities for Hitz VMs.
//! It leverages the `WinTun` driver to create virtual network adapters on the
//! Windows host, allowing the micro-VM to communicate with the outside world.
//!
//! It also provides bare-bones Ethernet frame parsing and ARP response logic,
//! bridging the gap between raw `WinTun` packets and the `virtio-net` device
//! backend located in `hitz-devices`.
//!
//! # The Hero's Journey
//!
//! Establishing a network link involves creating a `NetIoHandle` which manages
//! the `WinTun` adapter and spawns an I/O loop to shuttle packets between
//! the host interface and the guest's virtio queues.
//!
//! ```no_run
//! use hitz_api::NetConfig;
//! use hitz_net::start_net_io;
//!
//! // 1. Define network config
//! let cfg = NetConfig {
//!     mac: Some("02:00:00:00:00:01".into()),
//!     host_ip: "192.168.100.1/24".into(),
//!     guest_ip: "192.168.100.2/24".into(),
//!     adapter_name: Some("hitz-test".into()),
//! };
//!
//! // 2. The NetIoHandle will automatically create the adapter,
//! // assign the IP, and spawn the forwarding task when integrated
//! // with the VirtioNetDevice's queues.
//! let net_io = start_net_io(&cfg).expect("Failed to start network I/O");
//!
//! // 3. The guest can now use virtio-net to exchange ethernet frames.
//! ```
//!
//! # The Fine Print
//!
//! - **`WinTun` Dependency**: This crate relies heavily on `wintun` and the Windows
//!   Network API. It will fail to initialize if the `wintun.dll` is not present
//!   or if the user lacks administrator privileges required to create network adapters.
//! - **IP Forwarding**: Hitz configures the adapter, but the user is responsible for
//!   enabling IP forwarding or NAT via `PowerShell` (`New-NetNat`) if the guest needs
//!   internet access.
//!
//! ## Panics
//!
//! Parsing incorrectly formatted MAC addresses or invalid CIDR blocks will panic in certain
//! parsing paths. See [`parse_mac`] and [`parse_cidr`] for details on expected formats.

pub(crate) mod ethernet;
pub(crate) mod wintun_io;

pub use ethernet::{parse_cidr, parse_mac, random_mac};
pub use wintun_io::{NetIoError, NetIoHandle, start_net_io};
