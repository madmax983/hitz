with open('crates/hitz-api/src/prometheus.rs', 'r') as f:
    content = f.read()

correct_header = """//! Prometheus metrics exporter.
//!
//! # Abstract
//! This module provides a trait and implementation for converting hitz-api
//! telemetry (`MetricsSnapshot`) into the Prometheus text-based exposition format.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics, ToPrometheus};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1_700_000_000_000,
//!     cpu: CpuMetrics { total_pct: 12.5, per_core: vec![10.0, 15.0], load_avg: [0.5, 0.4, 0.3] },
//!     memory: MemoryMetrics {
//!         total_bytes: 256 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 156 * 1024 * 1024,
//!         buffers_bytes: 0,
//!         cached_bytes: 0,
//!         swap_total: 0,
//!         swap_used: 0,
//!     },
//!     disks: vec![],
//!     networks: vec![],
//!     processes: vec![],
//! };
//!
//! let prom_text = snap.to_prometheus("my_vm");
//! assert!(prom_text.contains("hitz_cpu_total_pct{vm_id=\\"my_vm\\"} 12.5 1700000000000"));
//! ```

use crate::MetricsSnapshot;

/// Trait to convert metrics into Prometheus text exposition format."""

idx = content.find("/// Trait to convert metrics into Prometheus text exposition format.")
if idx != -1:
    new_content = correct_header + content[idx + len("/// Trait to convert metrics into Prometheus text exposition format."):]
    with open('crates/hitz-api/src/prometheus.rs', 'w') as f:
        f.write(new_content)
