//! Mermaid Diagram Exporter Module
//!
//! # Abstract
//! Converts a `VmConfig` and `MetricsSnapshot` into a Mermaid.js diagram.

use crate::{MetricsSnapshot, VmConfig};
use std::fmt::Write as _;

/// Trait to export structures to Mermaid diagram.
pub trait ToMermaid {
    /// Returns the Mermaid graph as a String.
    fn to_mermaid(&self, resource_name: &str, snap: &MetricsSnapshot) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self, resource_name: &str, snap: &MetricsSnapshot) -> String {
        let mut mmd = String::new();
        let _ = writeln!(mmd, "graph TD");
        let _ = writeln!(mmd, "    subgraph {resource_name} [VM: {resource_name}]");

        let cpu_color = if snap.cpu.total_pct > 80.0 {
            "#ff6666"
        } else {
            "#99ff99"
        };
        let cpu_pct = snap.cpu.total_pct;
        let cpus = self.cpus;
        let _ = writeln!(
            mmd,
            "        CPU[CPU: {cpus} cores<br>{cpu_pct:.1}%]:::cpuStyle"
        );

        #[allow(clippy::cast_precision_loss)]
        let mem_pct = (snap.memory.used_bytes as f64 / snap.memory.total_bytes as f64) * 100.0;
        let mem_color = if mem_pct > 80.0 { "#ff6666" } else { "#99ff99" };
        let ram = self.ram_mib;
        let _ = writeln!(
            mmd,
            "        MEM[Memory: {ram} MiB<br>{mem_pct:.1}%]:::memStyle"
        );

        let _ = writeln!(mmd, "    end");
        let _ = writeln!(
            mmd,
            "    classDef cpuStyle fill:{cpu_color},stroke:#333,stroke-width:2px;"
        );
        let _ = writeln!(
            mmd,
            "    classDef memStyle fill:{mem_color},stroke:#333,stroke-width:2px;"
        );
        mmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics, MetricsSnapshot, VmConfig};
    use std::path::PathBuf;

    #[test]
    fn test_to_mermaid_basic() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 4,
            guest_agent: GuestAgentMode::Auto,
        };
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 85.0,
                per_core: vec![85.0, 85.0, 85.0, 85.0],
                load_avg: [1.2, 0.8, 0.5],
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
        let mmd = config.to_mermaid("test_vm", &snap);
        assert!(mmd.contains("graph TD"));
        assert!(mmd.contains("CPU: 4 cores"));
        assert!(mmd.contains("Memory: 1024 MiB"));
        // CPU should be red
        assert!(mmd.contains("fill:#ff6666"));
    }
}
