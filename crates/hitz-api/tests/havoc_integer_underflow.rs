#![allow(missing_docs, clippy::unwrap_used, unused_results)]

use hitz_api::{CalculateDiff, MetricsSnapshot, CpuMetrics, MemoryMetrics, FingerprintGenerator};
use hitz_api::simulator::{VmSimulator, WorkloadProfile};

#[test]
fn havoc_diff_panic_on_underflow_fixed() {
    let t1 = MetricsSnapshot {
        timestamp_ms: 1000,
        cpu: CpuMetrics { total_pct: 10.0, per_core: vec![10.0], load_avg: [0.1, 0.1, 0.1] },
        memory: MemoryMetrics { total_bytes: 1024, used_bytes: 512, free_bytes: 512, buffers_bytes: 0, cached_bytes: 0, swap_total: 0, swap_used: 0 },
        disks: vec![],
        networks: vec![],
        processes: vec![],
    };

    let mut t2 = t1.clone();
    t2.timestamp_ms = 500;

    let result = t2.diff(&t1);
    assert!(result.is_none());
}

#[test]
fn havoc_fingerprint_panic_on_underflow_fixed() {
    let used_mem: u64 = 200;
    let total_mem: u64 = 100;
    let snap = MetricsSnapshot {
        timestamp_ms: 1000,
        cpu: CpuMetrics { total_pct: 0.0, per_core: vec![0.0], load_avg: [0.1, 0.1, 0.1] },
        memory: MemoryMetrics {
            total_bytes: total_mem,
            used_bytes: used_mem,
            free_bytes: total_mem.saturating_sub(used_mem),
            buffers_bytes: 0,
            cached_bytes: 0,
            swap_total: 0,
            swap_used: 0,
        },
        disks: vec![],
        networks: vec![],
        processes: vec![],
    };
    let fp = snap.generate_fingerprint();
    assert_eq!(fp.id, "FP-C0-R10-D0-N0");
}

#[test]
fn havoc_simulator_panic_on_underflow_fixed() {
    let mut sim = VmSimulator::new(WorkloadProfile::MemoryLeak);
    for _ in 0..25 {
        sim.next().unwrap();
    }
}
