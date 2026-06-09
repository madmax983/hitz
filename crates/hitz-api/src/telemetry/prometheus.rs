//! Prometheus metrics exporter.
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
//! assert!(prom_text.contains("hitz_cpu_total_pct{vm_id=\"my_vm\"} 12.5 1700000000000"));
//! ```

use crate::MetricsSnapshot;

/// Trait to convert metrics into Prometheus text exposition format.
pub trait ToPrometheus {
    /// Convert the metrics snapshot to a Prometheus text string, tagging with the given `vm_id`.
    fn to_prometheus(&self, vm_id: &str) -> String;
}

impl ToPrometheus for MetricsSnapshot {
    #[allow(clippy::too_many_lines)]
    fn to_prometheus(&self, vm_id: &str) -> String {
        use core::fmt::Write;

        // Pre-allocate a reasonable buffer size to avoid dynamic reallocation.
        let mut out = String::with_capacity(2048);
        let ts = self.timestamp_ms;

        // Helper macro to append a metric line using `write!` to avoid intermediate String allocations.
        macro_rules! metric {
            ($name:expr, $type:expr, $help:expr, $val:expr) => {
                let _ = writeln!(out, "# HELP {} {}", $name, $help);
                let _ = writeln!(out, "# TYPE {} {}", $name, $type);
                let _ = writeln!(out, "{}{{vm_id=\"{}\"}} {} {}", $name, vm_id, $val, ts);
            };
            ($name:expr, $type:expr, $help:expr, $labels:expr, $val:expr) => {
                let _ = writeln!(out, "# HELP {} {}", $name, $help);
                let _ = writeln!(out, "# TYPE {} {}", $name, $type);
                let _ = writeln!(
                    out,
                    "{}{{vm_id=\"{}\",{}}} {} {}",
                    $name, vm_id, $labels, $val, ts
                );
            };
        }

        // ⚡ Bolt Optimization: Eliminated `format!` heap allocations for Prometheus labels.
        // We use `format_args!` instead, which passes the formatting arguments
        // directly to the underlying `writeln!` without creating intermediate `String`s.
        // --- CPU Metrics ---
        metric!(
            "hitz_cpu_total_pct",
            "gauge",
            "Overall CPU utilisation percentage",
            self.cpu.total_pct
        );

        for (i, core_pct) in self.cpu.per_core.iter().enumerate() {
            metric!(
                "hitz_cpu_core_pct",
                "gauge",
                "Per-core CPU utilisation percentage",
                format_args!("core=\"{}\"", i),
                core_pct
            );
        }

        metric!(
            "hitz_cpu_load_avg",
            "gauge",
            "System load average over 1 minute",
            "window=\"1m\"",
            self.cpu.load_avg[0]
        );
        metric!(
            "hitz_cpu_load_avg",
            "gauge",
            "System load average over 5 minutes",
            "window=\"5m\"",
            self.cpu.load_avg[1]
        );
        metric!(
            "hitz_cpu_load_avg",
            "gauge",
            "System load average over 15 minutes",
            "window=\"15m\"",
            self.cpu.load_avg[2]
        );

        // --- Memory Metrics ---
        metric!(
            "hitz_memory_total_bytes",
            "gauge",
            "Total physical memory in bytes",
            self.memory.total_bytes
        );
        metric!(
            "hitz_memory_used_bytes",
            "gauge",
            "Memory in use in bytes",
            self.memory.used_bytes
        );
        metric!(
            "hitz_memory_free_bytes",
            "gauge",
            "Free memory in bytes",
            self.memory.free_bytes
        );
        metric!(
            "hitz_memory_buffers_bytes",
            "gauge",
            "Memory used for buffers in bytes",
            self.memory.buffers_bytes
        );
        metric!(
            "hitz_memory_cached_bytes",
            "gauge",
            "Memory used for page cache in bytes",
            self.memory.cached_bytes
        );
        metric!(
            "hitz_memory_swap_total_bytes",
            "gauge",
            "Total swap space in bytes",
            self.memory.swap_total
        );
        metric!(
            "hitz_memory_swap_used_bytes",
            "gauge",
            "Swap space in use in bytes",
            self.memory.swap_used
        );

        // --- Disk Metrics ---
        for disk in &self.disks {
            metric!(
                "hitz_disk_reads_total",
                "counter",
                "Total completed read operations",
                format_args!("device=\"{}\"", disk.name),
                disk.reads_total
            );
            metric!(
                "hitz_disk_writes_total",
                "counter",
                "Total completed write operations",
                format_args!("device=\"{}\"", disk.name),
                disk.writes_total
            );
            metric!(
                "hitz_disk_read_bytes_total",
                "counter",
                "Total bytes read",
                format_args!("device=\"{}\"", disk.name),
                disk.read_bytes
            );
            metric!(
                "hitz_disk_write_bytes_total",
                "counter",
                "Total bytes written",
                format_args!("device=\"{}\"", disk.name),
                disk.write_bytes
            );
        }

        // --- Network Metrics ---
        for net in &self.networks {
            metric!(
                "hitz_net_rx_bytes_total",
                "counter",
                "Total bytes received",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_bytes
            );
            metric!(
                "hitz_net_tx_bytes_total",
                "counter",
                "Total bytes transmitted",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_bytes
            );
            metric!(
                "hitz_net_rx_packets_total",
                "counter",
                "Total packets received",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_packets
            );
            metric!(
                "hitz_net_tx_packets_total",
                "counter",
                "Total packets transmitted",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_packets
            );
            metric!(
                "hitz_net_rx_errors_total",
                "counter",
                "Total receive errors",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_errors
            );
            metric!(
                "hitz_net_tx_errors_total",
                "counter",
                "Total transmit errors",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_errors
            );
        }

        // --- Process Metrics ---
        for proc in &self.processes {
            metric!(
                "hitz_process_cpu_pct",
                "gauge",
                "CPU utilisation percentage for process",
                format_args!("pid=\"{}\",name=\"{}\"", proc.pid, proc.name),
                proc.cpu_pct
            );
            metric!(
                "hitz_process_rss_bytes",
                "gauge",
                "Resident set size in bytes for process",
                format_args!("pid=\"{}\",name=\"{}\"", proc.pid, proc.name),
                proc.rss_bytes
            );
        }

        out
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::expect_used)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics, ProcMetrics};

    #[test]
    fn test_to_prometheus() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1_700_000_000_000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 268_435_456,
                used_bytes: 104_857_600,
                free_bytes: 163_577_856,
                buffers_bytes: 1_048_576,
                cached_bytes: 2_097_152,
                swap_total: 10240,
                swap_used: 5120,
            },
            disks: vec![DiskMetrics {
                name: "vda".to_string(),
                reads_total: 100,
                writes_total: 50,
                read_bytes: 409_600,
                write_bytes: 204_800,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 1000,
                tx_bytes: 500,
                rx_packets: 10,
                tx_packets: 5,
                rx_errors: 1,
                tx_errors: 0,
            }],
            processes: vec![ProcMetrics {
                pid: 1,
                name: "systemd".to_string(),
                cpu_pct: 0.1,
                rss_bytes: 8_388_608,
                state: 'S',
            }],
        };

        let prom = snap.to_prometheus("test-vm");
        let ts = 1_700_000_000_000_u64;

        // Verify some expected strings
        let expected_lines = vec![
            format!("hitz_cpu_total_pct{{vm_id=\"test-vm\"}} 12.5 {ts}"),
            format!("hitz_cpu_core_pct{{vm_id=\"test-vm\",core=\"0\"}} 10 {ts}"),
            format!("hitz_cpu_core_pct{{vm_id=\"test-vm\",core=\"1\"}} 15 {ts}"),
            format!("hitz_cpu_load_avg{{vm_id=\"test-vm\",window=\"1m\"}} 0.5 {ts}"),
            format!("hitz_cpu_load_avg{{vm_id=\"test-vm\",window=\"5m\"}} 0.4 {ts}"),
            format!("hitz_cpu_load_avg{{vm_id=\"test-vm\",window=\"15m\"}} 0.3 {ts}"),
            format!("hitz_memory_total_bytes{{vm_id=\"test-vm\"}} 268435456 {ts}"),
            format!("hitz_memory_used_bytes{{vm_id=\"test-vm\"}} 104857600 {ts}"),
            format!("hitz_memory_free_bytes{{vm_id=\"test-vm\"}} 163577856 {ts}"),
            format!("hitz_memory_buffers_bytes{{vm_id=\"test-vm\"}} 1048576 {ts}"),
            format!("hitz_memory_cached_bytes{{vm_id=\"test-vm\"}} 2097152 {ts}"),
            format!("hitz_memory_swap_total_bytes{{vm_id=\"test-vm\"}} 10240 {ts}"),
            format!("hitz_memory_swap_used_bytes{{vm_id=\"test-vm\"}} 5120 {ts}"),
            format!("hitz_disk_reads_total{{vm_id=\"test-vm\",device=\"vda\"}} 100 {ts}"),
            format!("hitz_disk_writes_total{{vm_id=\"test-vm\",device=\"vda\"}} 50 {ts}"),
            format!("hitz_disk_read_bytes_total{{vm_id=\"test-vm\",device=\"vda\"}} 409600 {ts}"),
            format!("hitz_disk_write_bytes_total{{vm_id=\"test-vm\",device=\"vda\"}} 204800 {ts}"),
            format!("hitz_net_rx_bytes_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 1000 {ts}"),
            format!("hitz_net_tx_bytes_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 500 {ts}"),
            format!("hitz_net_rx_packets_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 10 {ts}"),
            format!("hitz_net_tx_packets_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 5 {ts}"),
            format!("hitz_net_rx_errors_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 1 {ts}"),
            format!("hitz_net_tx_errors_total{{vm_id=\"test-vm\",interface=\"eth0\"}} 0 {ts}"),
            format!(
                "hitz_process_cpu_pct{{vm_id=\"test-vm\",pid=\"1\",name=\"systemd\"}} 0.1 {ts}"
            ),
            format!(
                "hitz_process_rss_bytes{{vm_id=\"test-vm\",pid=\"1\",name=\"systemd\"}} 8388608 {ts}"
            ),
        ];

        for expected in expected_lines {
            assert!(
                prom.contains(&expected),
                "Missing expected line: {expected}"
            );
        }

        // Also verify that help and type headers are present
        assert!(prom.contains("# HELP hitz_cpu_total_pct"));
        assert!(prom.contains("# TYPE hitz_cpu_total_pct gauge"));
    }
}
