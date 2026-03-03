//! `WinTun` adapter management and network I/O thread.
//!
//! Manages the lifecycle of a `WinTun` virtual network adapter and runs
//! a background thread that bridges frames between the guest's virtio-net
//! device (via crossbeam channels) and the host network stack (via `WinTun`).
//!
//! Guest TX frames arrive as raw Ethernet:
//! - ARP requests for the gateway are replied to locally.
//! - IPv4/IPv6 frames have their Ethernet header stripped and are injected
//!   into the `WinTun` adapter as raw IP packets.
//!
//! Host-bound IP packets arriving on the `WinTun` adapter are wrapped in an
//! Ethernet frame (using the gateway MAC as source) and sent to the guest
//! via the RX channel.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::ethernet;

/// Handle to a running network I/O thread.
///
/// Keeps the thread alive until explicitly stopped via [`stop_and_join`]
/// or dropped. The [`Drop`] implementation signals the thread to stop
/// and joins it, ensuring clean shutdown.
///
/// [`stop_and_join`]: NetIoHandle::stop_and_join
pub struct NetIoHandle {
    /// The I/O thread handle. `None` after `stop_and_join` or `drop`.
    thread: Option<thread::JoinHandle<()>>,
    /// Shared stop flag; set to `true` to signal the I/O thread to exit.
    stop_flag: Arc<AtomicBool>,
    /// `WinTun` session; held here so `shutdown()` can be called on drop
    /// to unblock any `try_receive` / `receive_blocking` calls.
    session: Arc<wintun::Session>,
}

impl NetIoHandle {
    /// Signal the I/O thread to stop and wait for it to finish.
    pub fn stop_and_join(mut self) {
        self.shutdown_inner();
    }

    /// Internal shutdown: signal, unblock, join.
    fn shutdown_inner(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        // Unblock any blocking receive in the I/O thread.
        let _ = self.session.shutdown();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for NetIoHandle {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

/// Errors from network I/O operations.
#[derive(Debug, thiserror::Error)]
pub enum NetIoError {
    /// `WinTun` library or adapter error.
    #[error("WinTun error: {0}")]
    WinTun(String),
    /// Standard I/O error (thread spawn, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Configuration error (bad IP, CIDR, etc.).
    #[error("config error: {0}")]
    Config(String),
}

/// Start the network I/O thread.
///
/// Creates a `WinTun` adapter, configures the host-side IP address,
/// starts a `WinTun` session, and spawns a background thread that
/// bridges frames between the crossbeam channels and the adapter.
///
/// # Arguments
///
/// * `adapter_name` -- name for the `WinTun` adapter (e.g. "hitz-net")
/// * `host_ip` -- host-side IP in CIDR notation (e.g. "192.168.100.1/24")
/// * `guest_mac` -- the guest's MAC address (for building Ethernet frames)
/// * `gateway_mac` -- the virtual gateway MAC (for ARP replies and frame wrapping)
/// * `gateway_ip` -- the gateway's IPv4 address (4-byte array)
/// * `tx_receiver` -- receives raw Ethernet frames from the guest's TX path
/// * `rx_sender` -- sends raw Ethernet frames into the guest's RX path
///
/// # Errors
///
/// Returns [`NetIoError`] if `WinTun` cannot be loaded, the adapter cannot
/// be created, IP configuration fails, or the thread cannot be spawned.
#[allow(unsafe_code)]
pub fn start_net_io(
    adapter_name: &str,
    host_ip: &str,
    guest_mac: [u8; 6],
    gateway_mac: [u8; 6],
    gateway_ip: [u8; 4],
    tx_receiver: Receiver<Vec<u8>>,
    rx_sender: Sender<Vec<u8>>,
) -> Result<NetIoHandle, NetIoError> {
    // Load wintun.dll from the standard search path.
    // SAFETY: We trust that the wintun.dll in the system search path is
    // legitimate. The `wintun` crate marks `load()` as unsafe because
    // arbitrary DLL loading can execute attacker-controlled code, but
    // we accept this risk as WinTun is a well-known signed driver.
    let wintun = unsafe { wintun::load() }
        .map_err(|e| NetIoError::WinTun(format!("load wintun.dll: {e}")))?;

    // Create a new WinTun adapter.
    let adapter = wintun::Adapter::create(&wintun, adapter_name, "Hitz", None)
        .map_err(|e| NetIoError::WinTun(format!("create adapter '{adapter_name}': {e}")))?;

    // Configure the host-side IP address on the adapter.
    configure_host_ip(&adapter, host_ip)?;

    // Start a session with maximum ring capacity.
    let session = Arc::new(
        adapter
            .start_session(wintun::MAX_RING_CAPACITY)
            .map_err(|e| NetIoError::WinTun(format!("start session: {e}")))?,
    );

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop = stop_flag.clone();
    let session_clone = session.clone();
    let name = adapter_name.to_string();

    let thread = thread::Builder::new()
        .name(format!("hitz-net-{name}"))
        .spawn(move || {
            net_io_loop(
                &session_clone,
                &tx_receiver,
                &rx_sender,
                guest_mac,
                gateway_mac,
                gateway_ip,
                &stop,
            );
        })
        .map_err(NetIoError::Io)?;

    Ok(NetIoHandle {
        thread: Some(thread),
        stop_flag,
        session,
    })
}

/// Main I/O loop: bridges frames between crossbeam channels and `WinTun`.
///
/// Runs until `stop_flag` is set or an unrecoverable error occurs.
fn net_io_loop(
    session: &Arc<wintun::Session>,
    tx_receiver: &Receiver<Vec<u8>>,
    rx_sender: &Sender<Vec<u8>>,
    guest_mac: [u8; 6],
    gateway_mac: [u8; 6],
    gateway_ip: [u8; 4],
    stop_flag: &AtomicBool,
) {
    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // ── Guest TX: drain outgoing frames ──
        while let Ok(frame) = tx_receiver.try_recv() {
            if let Some(hdr) = ethernet::parse_eth_header(&frame) {
                match hdr.ethertype {
                    ethernet::ETHERTYPE_ARP => {
                        // Respond to ARP requests for the gateway locally.
                        if let Some(reply) =
                            ethernet::build_arp_reply(&frame, &gateway_mac, &gateway_ip)
                        {
                            let _ = rx_sender.send(reply);
                        }
                    }
                    ethernet::ETHERTYPE_IPV4 | ethernet::ETHERTYPE_IPV6 => {
                        // Strip Ethernet header, inject raw IP into the adapter.
                        if let Some(ip_packet) = ethernet::strip_eth_header(&frame) {
                            send_to_wintun(session, ip_packet);
                        }
                    }
                    _ => {
                        tracing::trace!(
                            ethertype = hdr.ethertype,
                            "dropping frame with unknown ethertype"
                        );
                    }
                }
            }
        }

        // ── Host RX: receive IP packets from the adapter ──
        match session.try_receive() {
            Ok(Some(packet)) => {
                let ip_data = packet.bytes();
                let ethertype = ethernet::ethertype_from_ip(ip_data);
                let frame = ethernet::build_eth_frame(&gateway_mac, &guest_mac, ethertype, ip_data);
                let _ = rx_sender.send(frame);
            }
            Ok(None) => {
                // No packet available; yield briefly to avoid busy-spinning.
                thread::sleep(Duration::from_millis(1));
            }
            Err(e) => {
                // Session may have been shut down.
                if !stop_flag.load(Ordering::Relaxed) {
                    tracing::error!("WinTun receive error: {e}");
                }
                break;
            }
        }
    }

    tracing::debug!("net I/O loop exiting");
}

/// Send a raw IP packet into the `WinTun` adapter.
///
/// Allocates a send packet, copies the data, and transmits it.
/// Errors are logged and silently dropped (best-effort networking).
fn send_to_wintun(session: &Arc<wintun::Session>, ip_packet: &[u8]) {
    let len = ip_packet.len();
    // `allocate_send_packet` takes a `u16`, so clamp to `u16::MAX`.
    // In practice, IP packets should never exceed 65535 bytes.
    #[allow(clippy::cast_possible_truncation)]
    let size: u16 = if len > usize::from(u16::MAX) {
        tracing::warn!(len, "IP packet too large for adapter, truncating");
        u16::MAX
    } else {
        len as u16
    };

    match session.allocate_send_packet(size) {
        Ok(mut wt_packet) => {
            let dest = wt_packet.bytes_mut();
            let copy_len = dest.len().min(ip_packet.len());
            dest[..copy_len].copy_from_slice(&ip_packet[..copy_len]);
            session.send_packet(wt_packet);
        }
        Err(e) => {
            tracing::debug!("allocate_send_packet failed: {e}");
        }
    }
}

/// Configure the host-side IP address on the `WinTun` adapter.
///
/// Uses the adapter's built-in `set_address` / `set_netmask` methods
/// rather than shelling out to `netsh`.
fn configure_host_ip(adapter: &Arc<wintun::Adapter>, cidr: &str) -> Result<(), NetIoError> {
    let (ip_bytes, prefix) = ethernet::parse_cidr(cidr).map_err(NetIoError::Config)?;
    let ip = Ipv4Addr::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);
    let mask = prefix_to_ipv4_mask(prefix);

    adapter
        .set_address(ip)
        .map_err(|e| NetIoError::WinTun(format!("set adapter address {ip}: {e}")))?;
    adapter
        .set_netmask(mask)
        .map_err(|e| NetIoError::WinTun(format!("set adapter netmask {mask}: {e}")))?;

    tracing::info!(%ip, %mask, "configured adapter IP");
    Ok(())
}

/// Convert a CIDR prefix length (0..=32) to an `Ipv4Addr` subnet mask.
fn prefix_to_ipv4_mask(prefix: u8) -> Ipv4Addr {
    let bits: u32 = if prefix == 0 {
        0
    } else if prefix >= 32 {
        0xFFFF_FFFF
    } else {
        !((1u32 << (32 - prefix)) - 1)
    };
    Ipv4Addr::from(bits.to_be_bytes())
}

/// Convert a CIDR prefix length to a dotted-decimal subnet mask string.
#[must_use]
pub fn prefix_to_mask(prefix: u8) -> String {
    let mask = prefix_to_ipv4_mask(prefix);
    mask.to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn prefix_to_mask_24() {
        assert_eq!(prefix_to_mask(24), "255.255.255.0");
    }

    #[test]
    fn prefix_to_mask_16() {
        assert_eq!(prefix_to_mask(16), "255.255.0.0");
    }

    #[test]
    fn prefix_to_mask_32() {
        assert_eq!(prefix_to_mask(32), "255.255.255.255");
    }

    #[test]
    fn prefix_to_mask_0() {
        assert_eq!(prefix_to_mask(0), "0.0.0.0");
    }

    #[test]
    fn prefix_to_mask_8() {
        assert_eq!(prefix_to_mask(8), "255.0.0.0");
    }

    #[test]
    fn prefix_to_ipv4_mask_roundtrip() {
        let mask = prefix_to_ipv4_mask(24);
        assert_eq!(mask, Ipv4Addr::new(255, 255, 255, 0));
    }
}
