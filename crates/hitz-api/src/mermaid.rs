//! Mermaid.js Exporter Module
//!
//! # Abstract
//! Converts a `MetricsSnapshot` into a Mermaid.js flowchart, allowing users to
//! visually inspect VM health and resource distribution.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics};
//! use hitz_api::ToMermaid;
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 25.0,
//!         per_core: vec![50.0, 0.0],
//!         load_avg: [1.0, 0.5, 0.2],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 512 * 1024 * 1024,
//!         free_bytes: 512 * 1024 * 1024,
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
//! let chart = snap.to_mermaid("VM Architecture");
//! assert!(chart.contains("graph TD"));
//! assert!(chart.contains("CPU[CPU Usage: 25.0%]"));
//! ```

use crate::MetricsSnapshot;
use std::fmt::Write as _;

/// Trait to export structures to a Mermaid.js flowchart.
pub trait ToMermaid {
    /// Returns the Mermaid.js representation as a String.
    fn to_mermaid(&self, title: &str) -> String;
}

impl ToMermaid for MetricsSnapshot {
    fn to_mermaid(&self, title: &str) -> String {
        let mut chart = String::with_capacity(512);
        let _ = writeln!(chart, "---");
        let _ = writeln!(chart, "title: {title}");
        let _ = writeln!(chart, "---");
        let _ = writeln!(chart, "graph TD");
        let _ = writeln!(chart, "  VM((Micro-VM))");
        let _ = writeln!(chart, "  VM --> CPU[CPU Usage: {:.1}%]", self.cpu.total_pct);
        let _ = writeln!(
            chart,
            "  VM --> RAM[RAM Used: {} bytes]",
            self.memory.used_bytes
        );

        for disk in &self.disks {
            let _ = writeln!(
                chart,
                "  VM --> DISK_{}[Disk {}: {} read, {} write]",
                disk.name, disk.name, disk.read_bytes, disk.write_bytes
            );
        }

        for net in &self.networks {
            let _ = writeln!(
                chart,
                "  VM --> NET_{}[Net {}: {} rx, {} tx]",
                net.interface, net.interface, net.rx_bytes, net.tx_bytes
            );
        }

        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, MemoryMetrics};

    #[test]
    fn test_to_mermaid_basic() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 25.0,
                per_core: vec![50.0, 0.0],
                load_avg: [1.0, 0.5, 0.2],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024 * 1024,
                used_bytes: 512 * 1024 * 1024,
                free_bytes: 512 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let chart = snap.to_mermaid("Test VM");
        assert!(chart.contains("graph TD"));
        assert!(chart.contains("CPU[CPU Usage: 25.0%]"));
        assert!(chart.contains("RAM[RAM Used: 536870912 bytes]"));
    }
}
