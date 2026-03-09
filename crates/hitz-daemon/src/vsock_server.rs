//! Host-side virtio-vsock metrics receiver.
//!
//! Reads [`MetricsSnapshot`] packets pushed by the guest agent over vsock
//! and publishes them to the global OpenTelemetry meter.

use crossbeam_channel::{Receiver, Sender};
use hitz_api::{MetricsSnapshot, VSOCK_METRICS_PORT};
use hitz_devices::virtio::vsock::{VSOCK_BUF_ALLOC, VsockHdr, VsockOp, VsockPacket};
use opentelemetry::KeyValue;
use tokio::sync::watch;

/// Receive loop for a single VM's vsock metrics stream.
///
/// Runs until the channels close (VM exit) or `shutdown` fires.
pub async fn run_metrics_task(
    vm_id: String,
    tx_rx: Receiver<VsockPacket>,
    rx_tx: Sender<VsockPacket>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut fwd_cnt: u32 = 0;
    loop {
        // Yield control briefly so the tokio runtime can schedule other tasks.
        tokio::task::yield_now().await;

        // Check shutdown first.
        if *shutdown.borrow() {
            break;
        }

        // Non-blocking receive: if the channel has a packet, process it.
        // If empty, check shutdown and loop again.
        match tx_rx.try_recv() {
            Ok((hdr, payload)) => {
                handle_packet(&vm_id, &hdr, &payload, &rx_tx, &mut fwd_cnt);
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                // No packet yet — wait for either a packet or shutdown.
                let tx_rx_clone = tx_rx.clone();
                let packet_result = tokio::select! {
                    packet = tokio::task::spawn_blocking(move || tx_rx_clone.recv()) => {
                        packet
                    }
                    changed = shutdown.changed() => {
                        let _ = changed;
                        break;
                    }
                };
                match packet_result {
                    Ok(Ok((hdr, payload))) => {
                        handle_packet(&vm_id, &hdr, &payload, &rx_tx, &mut fwd_cnt);
                    }
                    _ => break,
                }
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => break,
        }
    }
}

fn handle_packet(
    vm_id: &str,
    hdr: &VsockHdr,
    payload: &[u8],
    rx_tx: &Sender<VsockPacket>,
    fwd_cnt: &mut u32,
) {
    match VsockOp::from_u16(hdr.op) {
        Some(VsockOp::Request) => {
            let resp = hdr.make_response(VsockOp::Response, 0);
            let _ = rx_tx.try_send((resp, vec![]));
        }
        Some(VsockOp::Rw) if hdr.dst_port == VSOCK_METRICS_PORT => {
            if payload.len() >= 4 {
                let len = u32::from_le_bytes(payload[0..4].try_into().unwrap_or([0; 4])) as usize;
                if payload.len() >= 4 + len
                    && let Ok(snap) = rmp_serde::from_slice::<MetricsSnapshot>(&payload[4..4 + len])
                {
                    publish_to_otel(vm_id, &snap);
                }
            }
            *fwd_cnt = fwd_cnt.saturating_add(hdr.len);
            let mut credit = hdr.make_response(VsockOp::CreditUpdate, 0);
            credit.fwd_cnt = *fwd_cnt;
            credit.buf_alloc = VSOCK_BUF_ALLOC;
            let _ = rx_tx.try_send((credit, vec![]));
        }
        Some(VsockOp::Shutdown | VsockOp::Rst) => {
            let rst = hdr.make_response(VsockOp::Rst, 0);
            let _ = rx_tx.try_send((rst, vec![]));
        }
        _ => {}
    }
}

/// Publish a [`MetricsSnapshot`] to the global `OTel` meter.
///
/// All calls are no-ops when no `OTel` provider is registered.
pub fn publish_to_otel(vm_id: &str, snap: &MetricsSnapshot) {
    let meter = opentelemetry::global::meter("hitz");
    let labels = [KeyValue::new("vm.id", vm_id.to_string())];

    meter
        .f64_gauge("hitz.guest.cpu_usage")
        .with_description("Guest overall CPU utilisation percentage")
        .build()
        .record(f64::from(snap.cpu.total_pct), &labels);

    for (i, &pct) in snap.cpu.per_core.iter().enumerate() {
        let core_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("cpu", i.to_string()),
        ];
        meter
            .f64_gauge("hitz.guest.cpu_usage")
            .build()
            .record(f64::from(pct), &core_labels);
    }

    meter
        .u64_gauge("hitz.guest.memory_used_bytes")
        .with_description("Guest memory in use")
        .build()
        .record(snap.memory.used_bytes, &labels);

    meter
        .u64_gauge("hitz.guest.memory_total_bytes")
        .with_description("Guest total RAM")
        .build()
        .record(snap.memory.total_bytes, &labels);

    for disk in &snap.disks {
        let disk_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("disk", disk.name.clone()),
        ];
        meter
            .u64_counter("hitz.guest.disk_read_bytes_total")
            .build()
            .add(disk.read_bytes, &disk_labels);
        meter
            .u64_counter("hitz.guest.disk_write_bytes_total")
            .build()
            .add(disk.write_bytes, &disk_labels);
    }

    for net in &snap.networks {
        let net_labels = [
            KeyValue::new("vm.id", vm_id.to_string()),
            KeyValue::new("interface", net.interface.clone()),
        ];
        meter
            .u64_counter("hitz.guest.net_rx_bytes_total")
            .build()
            .add(net.rx_bytes, &net_labels);
        meter
            .u64_counter("hitz.guest.net_tx_bytes_total")
            .build()
            .add(net.tx_bytes, &net_labels);
    }

    tracing::debug!(
        vm.id = vm_id,
        cpu_pct = snap.cpu.total_pct,
        mem_used = snap.memory.used_bytes,
        "guest metrics snapshot received"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hitz_api::{CpuMetrics, MemoryMetrics, MetricsSnapshot};

    #[test]
    fn publish_to_otel_does_not_panic() {
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 50.0,
                per_core: vec![50.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 128 * 1024 * 1024,
                free_bytes: 128 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        publish_to_otel("test-vm", &snap);
    }
}
