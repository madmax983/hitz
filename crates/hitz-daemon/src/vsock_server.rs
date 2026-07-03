//! Host-side virtio-vsock metrics receiver.
//!
//! Reads [`MetricsSnapshot`] packets pushed by the guest agent over vsock
//! and publishes them to the global OpenTelemetry meter.

use crossbeam_channel::{Receiver, Sender};
use hitz_api::{MetricsSnapshot, VSOCK_METRICS_PORT};
use hitz_devices::{VSOCK_BUF_ALLOC, VsockHdr, VsockOp, VsockPacket};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge};
use std::sync::OnceLock;
use tokio::sync::watch;

struct OTelInstruments {
    cpu_usage: Gauge<f64>,
    cpu_usage_per_core: Gauge<f64>,
    memory_used_bytes: Gauge<u64>,
    memory_total_bytes: Gauge<u64>,
    disk_read_bytes_total: Counter<u64>,
    disk_write_bytes_total: Counter<u64>,
    net_rx_bytes_total: Counter<u64>,
    net_tx_bytes_total: Counter<u64>,
}

static OTEL_INSTRUMENTS: OnceLock<OTelInstruments> = OnceLock::new();

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
            if let Some(len_bytes) = payload.get(0..4) {
                let len = u32::from_le_bytes(len_bytes.try_into().unwrap_or([0; 4])) as usize;
                if let Some(snap_bytes) = payload.get(4..4 + len) {
                    if let Ok(snap) = rmp_serde::from_slice::<MetricsSnapshot>(snap_bytes) {
                        publish_to_otel(vm_id, &snap);
                    }
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
    let insts = OTEL_INSTRUMENTS.get_or_init(|| {
        let meter = opentelemetry::global::meter("hitz");
        OTelInstruments {
            cpu_usage: meter
                .f64_gauge("hitz.guest.cpu_usage")
                .with_description("Guest overall CPU utilisation percentage")
                .build(),
            cpu_usage_per_core: meter.f64_gauge("hitz.guest.cpu_usage_per_core").build(),
            memory_used_bytes: meter
                .u64_gauge("hitz.guest.memory_used_bytes")
                .with_description("Guest memory in use")
                .build(),
            memory_total_bytes: meter
                .u64_gauge("hitz.guest.memory_total_bytes")
                .with_description("Guest total RAM")
                .build(),
            disk_read_bytes_total: meter
                .u64_counter("hitz.guest.disk_read_bytes_total")
                .build(),
            disk_write_bytes_total: meter
                .u64_counter("hitz.guest.disk_write_bytes_total")
                .build(),
            net_rx_bytes_total: meter.u64_counter("hitz.guest.net_rx_bytes_total").build(),
            net_tx_bytes_total: meter.u64_counter("hitz.guest.net_tx_bytes_total").build(),
        }
    });

    // ⚡ Bolt Optimization:
    // We pre-allocate the `vm.id` KeyValue to avoid `vm_id.to_string()` heap allocations
    // inside the loops for cores, disks, and network interfaces. We also replace
    // `i.to_string()` with `i as i64` for CPU cores to prevent allocation per core.
    let vm_id_kv = KeyValue::new("vm.id", vm_id.to_string());
    let labels = [vm_id_kv.clone()];

    insts
        .cpu_usage
        .record(f64::from(snap.cpu.total_pct), &labels);

    for (i, &pct) in snap.cpu.per_core.iter().enumerate() {
        let core_labels = [vm_id_kv.clone(), KeyValue::new("cpu", i as i64)];
        insts
            .cpu_usage_per_core
            .record(f64::from(pct), &core_labels);
    }

    insts
        .memory_used_bytes
        .record(snap.memory.used_bytes, &labels);
    insts
        .memory_total_bytes
        .record(snap.memory.total_bytes, &labels);

    for disk in &snap.disks {
        let disk_labels = [vm_id_kv.clone(), KeyValue::new("disk", disk.name.clone())];
        insts
            .disk_read_bytes_total
            .add(disk.read_bytes, &disk_labels);
        insts
            .disk_write_bytes_total
            .add(disk.write_bytes, &disk_labels);
    }

    for net in &snap.networks {
        let net_labels = [
            vm_id_kv.clone(),
            KeyValue::new("interface", net.interface.clone()),
        ];
        insts.net_rx_bytes_total.add(net.rx_bytes, &net_labels);
        insts.net_tx_bytes_total.add(net.tx_bytes, &net_labels);
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

    #[test]
    fn test_handle_packet() {
        struct TestCase {
            name: &'static str,
            op: u16,
            dst_port: u32,
            payload: Vec<u8>,
            expected_op: Option<VsockOp>,
        }

        let cases = vec![
            TestCase {
                name: "Request should get Response",
                op: VsockOp::Request as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![],
                expected_op: Some(VsockOp::Response),
            },
            TestCase {
                name: "Rw to wrong port is ignored",
                op: VsockOp::Rw as u16,
                dst_port: 9999, // wrong port
                payload: vec![],
                expected_op: None,
            },
            TestCase {
                name: "Rw to correct port gets CreditUpdate",
                op: VsockOp::Rw as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![], // empty payload won't panic, handled safely
                expected_op: Some(VsockOp::CreditUpdate),
            },
            TestCase {
                name: "Rw short payload (<4 bytes)",
                op: VsockOp::Rw as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![1, 2], // 2 bytes instead of 4
                expected_op: Some(VsockOp::CreditUpdate),
            },
            TestCase {
                name: "Rw mismatch payload length",
                op: VsockOp::Rw as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![100, 0, 0, 0, 1, 2, 3], // Header says 100, but only 3 bytes data
                expected_op: Some(VsockOp::CreditUpdate),
            },
            TestCase {
                name: "Shutdown gets Rst",
                op: VsockOp::Shutdown as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![],
                expected_op: Some(VsockOp::Rst),
            },
            TestCase {
                name: "Rst gets Rst",
                op: VsockOp::Rst as u16,
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![],
                expected_op: Some(VsockOp::Rst),
            },
            TestCase {
                name: "Unknown op gets ignored",
                op: 999, // invalid op
                dst_port: VSOCK_METRICS_PORT,
                payload: vec![],
                expected_op: None,
            },
        ];

        for case in cases {
            let (rx_tx, rx_rx) = crossbeam_channel::unbounded();
            let mut fwd_cnt = 0;
            // Create a default header using from_bytes
            #[allow(clippy::unwrap_used)]
            let mut hdr = VsockHdr::from_bytes(&[0; 44]).expect("vsock read failed");
            hdr.op = case.op;
            hdr.dst_port = case.dst_port;
            hdr.len = u32::try_from(case.payload.len()).unwrap_or(0);

            handle_packet("test-vm", &hdr, &case.payload, &rx_tx, &mut fwd_cnt);

            if let Some(expected_op) = case.expected_op {
                #[allow(clippy::expect_used)]
                let response = rx_rx
                    .try_recv()
                    .unwrap_or_else(|_| panic!("{}: expected response", case.name));
                let (res_hdr, _) = response;
                assert_eq!(
                    res_hdr.op, expected_op as u16,
                    "{}: response op mismatch",
                    case.name
                );
            } else {
                assert!(
                    rx_rx.try_recv().is_err(),
                    "{}: expected no response",
                    case.name
                );
            }
        }
    }
}
