//! Prometheus exposition format exporter for Hitz metrics.
//!
//! # Abstract
//! This module provides a way to convert our structured telemetry
//! ([`MetricsSnapshot`]) into the text-based exposition format expected by
//! Prometheus scrapers.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{MetricsSnapshot, CpuMetrics, MemoryMetrics, ToPrometheus};
//!
//! let snapshot = MetricsSnapshot {
//!     timestamp_ms: 1610000000000,
//!     cpu: CpuMetrics {
//!         total_pct: 12.5,
//!         per_core: vec![10.0, 15.0],
//!         load_avg: [0.1, 0.2, 0.3],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024,
//!         used_bytes: 512,
//!         free_bytes: 512,
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
//! let prom_text = snapshot.to_prometheus("my_vm_1");
//! assert!(prom_text.contains("hitz_cpu_total_pct{vm_id=\"my_vm_1\"} 12.5"));
//! ```

use crate::MetricsSnapshot;
use std::fmt::Write;

/// A trait for types that can be exported to Prometheus exposition format.
///
/// # Abstract
/// This trait provides a standardized interface for converting structured
/// metrics into the plain-text format that Prometheus expects when it scrapes
/// a `/metrics` endpoint.
pub trait ToPrometheus {
    /// Renders the object as a Prometheus-compatible string.
    ///
    /// # Arguments
    ///
    /// * `vm_id` - The identifier of the VM, attached as a label to all metrics.
    fn to_prometheus(&self, vm_id: &str) -> String;
}

impl ToPrometheus for MetricsSnapshot {
    #[allow(clippy::too_many_lines)]
    fn to_prometheus(&self, vm_id: &str) -> String {
        // Pre-allocate a string with a reasonable capacity to avoid
        // multiple reallocations while formatting the metrics.
        let mut out = String::with_capacity(1024);

        // -- CPU --
        let _ = writeln!(
            &mut out,
            "# HELP hitz_cpu_total_pct Overall CPU utilization percentage"
        );
        let _ = writeln!(&mut out, "# TYPE hitz_cpu_total_pct gauge");
        let _ = writeln!(
            &mut out,
            "hitz_cpu_total_pct{{vm_id=\"{vm_id}\"}} {}",
            self.cpu.total_pct
        );

        let _ = writeln!(
            &mut out,
            "# HELP hitz_cpu_core_pct Per-core CPU utilization percentage"
        );
        let _ = writeln!(&mut out, "# TYPE hitz_cpu_core_pct gauge");
        for (i, pct) in self.cpu.per_core.iter().enumerate() {
            let _ = writeln!(
                &mut out,
                "hitz_cpu_core_pct{{vm_id=\"{vm_id}\",core=\"{i}\"}} {pct}"
            );
        }

        let _ = writeln!(
            &mut out,
            "# HELP hitz_cpu_load_avg System load average (1m, 5m, 15m)"
        );
        let _ = writeln!(&mut out, "# TYPE hitz_cpu_load_avg gauge");
        let _ = writeln!(
            &mut out,
            "hitz_cpu_load_avg{{vm_id=\"{vm_id}\",period=\"1m\"}} {}",
            self.cpu.load_avg[0]
        );
        let _ = writeln!(
            &mut out,
            "hitz_cpu_load_avg{{vm_id=\"{vm_id}\",period=\"5m\"}} {}",
            self.cpu.load_avg[1]
        );
        let _ = writeln!(
            &mut out,
            "hitz_cpu_load_avg{{vm_id=\"{vm_id}\",period=\"15m\"}} {}",
            self.cpu.load_avg[2]
        );

        // -- Memory --
        let _ = writeln!(
            &mut out,
            "# HELP hitz_memory_bytes Memory utilization in bytes"
        );
        let _ = writeln!(&mut out, "# TYPE hitz_memory_bytes gauge");
        let _ = writeln!(
            &mut out,
            "hitz_memory_bytes{{vm_id=\"{vm_id}\",type=\"total\"}} {}",
            self.memory.total_bytes
        );
        let _ = writeln!(
            &mut out,
            "hitz_memory_bytes{{vm_id=\"{vm_id}\",type=\"used\"}} {}",
            self.memory.used_bytes
        );
        let _ = writeln!(
            &mut out,
            "hitz_memory_bytes{{vm_id=\"{vm_id}\",type=\"free\"}} {}",
            self.memory.free_bytes
        );
        let _ = writeln!(
            &mut out,
            "hitz_memory_bytes{{vm_id=\"{vm_id}\",type=\"buffers\"}} {}",
            self.memory.buffers_bytes
        );
        let _ = writeln!(
            &mut out,
            "hitz_memory_bytes{{vm_id=\"{vm_id}\",type=\"cached\"}} {}",
            self.memory.cached_bytes
        );

        let _ = writeln!(&mut out, "# HELP hitz_swap_bytes Swap utilization in bytes");
        let _ = writeln!(&mut out, "# TYPE hitz_swap_bytes gauge");
        let _ = writeln!(
            &mut out,
            "hitz_swap_bytes{{vm_id=\"{vm_id}\",type=\"total\"}} {}",
            self.memory.swap_total
        );
        let _ = writeln!(
            &mut out,
            "hitz_swap_bytes{{vm_id=\"{vm_id}\",type=\"used\"}} {}",
            self.memory.swap_used
        );

        // -- Disks --
        if !self.disks.is_empty() {
            let _ = writeln!(&mut out, "# HELP hitz_disk_reads_total Total disk reads");
            let _ = writeln!(&mut out, "# TYPE hitz_disk_reads_total counter");
            for disk in &self.disks {
                let _ = writeln!(
                    &mut out,
                    "hitz_disk_reads_total{{vm_id=\"{vm_id}\",device=\"{}\"}} {}",
                    disk.name, disk.reads_total
                );
            }

            let _ = writeln!(&mut out, "# HELP hitz_disk_writes_total Total disk writes");
            let _ = writeln!(&mut out, "# TYPE hitz_disk_writes_total counter");
            for disk in &self.disks {
                let _ = writeln!(
                    &mut out,
                    "hitz_disk_writes_total{{vm_id=\"{vm_id}\",device=\"{}\"}} {}",
                    disk.name, disk.writes_total
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_disk_read_bytes_total Total disk read bytes"
            );
            let _ = writeln!(&mut out, "# TYPE hitz_disk_read_bytes_total counter");
            for disk in &self.disks {
                let _ = writeln!(
                    &mut out,
                    "hitz_disk_read_bytes_total{{vm_id=\"{vm_id}\",device=\"{}\"}} {}",
                    disk.name, disk.read_bytes
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_disk_write_bytes_total Total disk write bytes"
            );
            let _ = writeln!(&mut out, "# TYPE hitz_disk_write_bytes_total counter");
            for disk in &self.disks {
                let _ = writeln!(
                    &mut out,
                    "hitz_disk_write_bytes_total{{vm_id=\"{vm_id}\",device=\"{}\"}} {}",
                    disk.name, disk.write_bytes
                );
            }
        }

        // -- Networks --
        if !self.networks.is_empty() {
            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_receive_bytes_total Total network bytes received"
            );
            let _ = writeln!(&mut out, "# TYPE hitz_network_receive_bytes_total counter");
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_receive_bytes_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.rx_bytes
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_transmit_bytes_total Total network bytes transmitted"
            );
            let _ = writeln!(&mut out, "# TYPE hitz_network_transmit_bytes_total counter");
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_transmit_bytes_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.tx_bytes
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_receive_packets_total Total network packets received"
            );
            let _ = writeln!(
                &mut out,
                "# TYPE hitz_network_receive_packets_total counter"
            );
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_receive_packets_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.rx_packets
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_transmit_packets_total Total network packets transmitted"
            );
            let _ = writeln!(
                &mut out,
                "# TYPE hitz_network_transmit_packets_total counter"
            );
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_transmit_packets_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.tx_packets
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_receive_errors_total Total network receive errors"
            );
            let _ = writeln!(&mut out, "# TYPE hitz_network_receive_errors_total counter");
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_receive_errors_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.rx_errors
                );
            }

            let _ = writeln!(
                &mut out,
                "# HELP hitz_network_transmit_errors_total Total network transmit errors"
            );
            let _ = writeln!(
                &mut out,
                "# TYPE hitz_network_transmit_errors_total counter"
            );
            for net in &self.networks {
                let _ = writeln!(
                    &mut out,
                    "hitz_network_transmit_errors_total{{vm_id=\"{vm_id}\",interface=\"{}\"}} {}",
                    net.interface, net.tx_errors
                );
            }
        }

        out
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics, ProcMetrics};

    fn dummy_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1610000000000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.1, 0.2, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 1024,
                used_bytes: 512,
                free_bytes: 512,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".to_string(),
                reads_total: 100,
                writes_total: 50,
                read_bytes: 4096,
                write_bytes: 2048,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 10240,
                tx_bytes: 5120,
                rx_packets: 10,
                tx_packets: 5,
                rx_errors: 1,
                tx_errors: 0,
            }],
            processes: vec![ProcMetrics {
                pid: 1,
                name: "init".to_string(),
                cpu_pct: 0.1,
                rss_bytes: 1024,
                state: 'S',
            }],
        }
    }

    #[test]
    fn test_to_prometheus() {
        let snap = dummy_snapshot();
        let prom = snap.to_prometheus("my_vm");

        // Verify some expected strings
        assert!(prom.contains("hitz_cpu_total_pct{vm_id=\"my_vm\"} 12.5"));
        assert!(prom.contains("hitz_cpu_core_pct{vm_id=\"my_vm\",core=\"0\"} 10"));
        assert!(prom.contains("hitz_cpu_core_pct{vm_id=\"my_vm\",core=\"1\"} 15"));
        assert!(prom.contains("hitz_cpu_load_avg{vm_id=\"my_vm\",period=\"1m\"} 0.1"));
        assert!(prom.contains("hitz_cpu_load_avg{vm_id=\"my_vm\",period=\"15m\"} 0.3"));

        assert!(prom.contains("hitz_memory_bytes{vm_id=\"my_vm\",type=\"used\"} 512"));
        assert!(prom.contains("hitz_swap_bytes{vm_id=\"my_vm\",type=\"total\"} 0"));

        assert!(prom.contains("hitz_disk_reads_total{vm_id=\"my_vm\",device=\"vda\"} 100"));
        assert!(prom.contains("hitz_disk_write_bytes_total{vm_id=\"my_vm\",device=\"vda\"} 2048"));

        assert!(prom.contains(
            "hitz_network_receive_bytes_total{vm_id=\"my_vm\",interface=\"eth0\"} 10240"
        ));
        assert!(
            prom.contains(
                "hitz_network_receive_errors_total{vm_id=\"my_vm\",interface=\"eth0\"} 1"
            )
        );
    }
}
