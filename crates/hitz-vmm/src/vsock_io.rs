//! Channel handle bridging the vsock virtio device (sync run-loop)
//! with the host-side async metrics runtime.
//!
//! # Abstract
//! Provides the data structures required to move data across the sync/async boundary
//! for the `VirtioVsockDevice`.
//!
//! The daemon creates a [`VsockIoHandle`] before calling `spawn_blocking`
//! for `boot_and_run`, retaining it so the async task can send/receive
//! vsock packets while the VM is running. Dropping the handle closes
//! the channels, which signals the device's poll loop to stop injecting
//! and draining packets.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_vmm::VsockIoHandle;
//!
//! // The daemon provisions the channel pairs before starting the VM
//! let (handle, rx_recv, tx_send) = VsockIoHandle::new_pair();
//!
//! // The handle is kept in the async runtime...
//! assert!(handle.tx_rx.is_empty());
//!
//! // ...and the other ends are passed to `boot_and_run`.
//! assert!(rx_recv.is_empty());
//! ```

use hitz_devices::VsockPacket;

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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use hitz_devices::VsockHdr;

    #[test]
    #[allow(clippy::expect_used)]
    fn should_create_matched_pairs_and_route_messages_correctly() {
        let (handle, rx_receiver, tx_sender) = VsockIoHandle::new_pair();

        let dummy_hdr = VsockHdr {
            src_cid: 1,
            dst_cid: 2,
            src_port: 3,
            dst_port: 4,
            len: 0,
            r#type: 1,
            op: 1,
            flags: 0,
            buf_alloc: 1024,
            fwd_cnt: 0,
        };

        // 1. Host sends to Device (rx direction)
        handle
            .rx_tx
            .send((dummy_hdr.clone(), vec![]))
            .expect("failed to send host->device");

        let (received_hdr, payload) = rx_receiver
            .try_recv()
            .expect("failed to receive on device rx");
        assert_eq!(received_hdr.src_cid, 1, "mismatched src_cid");
        assert_eq!(received_hdr.dst_cid, 2, "mismatched dst_cid");
        assert!(payload.is_empty(), "expected empty payload");

        // 2. Device sends to Host (tx direction)
        tx_sender
            .send((dummy_hdr, vec![1, 2, 3]))
            .expect("failed to send device->host");

        let (received_hdr, payload) = handle
            .tx_rx
            .try_recv()
            .expect("failed to receive on host tx");
        assert_eq!(received_hdr.src_cid, 1, "mismatched src_cid");
        assert_eq!(received_hdr.dst_cid, 2, "mismatched dst_cid");
        assert_eq!(payload, vec![1, 2, 3], "mismatched payload");
    }
}
