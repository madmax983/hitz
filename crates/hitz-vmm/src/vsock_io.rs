//! Channel handle bridging the vsock virtio device (sync run-loop)
//! with the host-side async metrics runtime.
//!
//! The daemon creates a [`VsockIoHandle`] before calling `spawn_blocking`
//! for `boot_and_run`, retaining it so the async task can send/receive
//! vsock packets while the VM is running. Dropping the handle closes
//! the channels, which signals the device's poll loop to stop injecting
//! and draining packets.

use hitz_devices::virtio::vsock::VsockPacket;

/// Owned channel endpoints for the host side of a vsock device.
///
/// The device-facing ends (opposite to these) are passed to `boot_and_run`
/// via [`crate::vm::BootExtras`] and wired into the `VirtioVsockDevice`.
///
/// Drop this handle to close the channels and signal the run-loop that
/// vsock I/O should stop (the device's `try_send`/`try_recv` will return
/// `Err` once both ends are gone).
pub struct VsockIoHandle {
    /// Receive guest→host packets (drained from the TX virtqueue by the device).
    pub tx_rx: crossbeam_channel::Receiver<VsockPacket>,
    /// Send host→guest packets (injected into the RX virtqueue by the device).
    pub rx_tx: crossbeam_channel::Sender<VsockPacket>,
}

impl VsockIoHandle {
    /// Create a matched pair: a `VsockIoHandle` for the host side and the
    /// device-facing channel ends to pass to `boot_and_run` via `BootExtras`.
    ///
    /// ```text
    /// Host (VsockIoHandle)           Device (BootExtras / VirtioVsockDevice)
    ///   rx_tx  ──────────────────────>  rx_receiver  (injects RX into guest)
    ///   tx_rx  <──────────────────────  tx_sender    (drains TX from guest)
    /// ```
    #[must_use]
    pub fn new_pair() -> (
        Self,
        crossbeam_channel::Receiver<VsockPacket>,
        crossbeam_channel::Sender<VsockPacket>,
    ) {
        // tx channel: device→host (guest TX queue → host reader)
        let (tx_sender, tx_rx) = crossbeam_channel::unbounded::<VsockPacket>();
        // rx channel: host→device (host writer → guest RX queue)
        let (rx_tx, rx_receiver) = crossbeam_channel::unbounded::<VsockPacket>();

        let handle = Self { tx_rx, rx_tx };
        // Return device-facing ends in the same order as BootExtras.vsock_channels:
        // (rx_receiver, tx_sender)
        (handle, rx_receiver, tx_sender)
    }
}
