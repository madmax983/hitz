//! Mermaid chart generation module.
//!
//! # Abstract
//! Converts a `VmConfig` or `MetricsSnapshot` into a Mermaid chart string,
//! allowing users to visually represent their VM configurations or resource usage.
//!
//! # The Hero's Journey
//! ```rust
//! use hitz_api::{VmConfig, GuestAgentMode};
//! #[cfg(feature = "mermaid")]
//! use hitz_api::ToMermaid;
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("/boot/vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 4,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! # #[cfg(feature = "mermaid")]
//! # {
//! let chart = config.to_mermaid();
//! assert!(chart.contains("graph TD;"));
//! # }
//! ```

use crate::{MetricsSnapshot, VmConfig};
use std::fmt::Write as _;

/// Trait to export structures to Mermaid chart representation.
pub trait ToMermaid {
    /// Returns the Mermaid chart representation as a String.
    fn to_mermaid(&self) -> String;
}

impl ToMermaid for VmConfig {
    fn to_mermaid(&self) -> String {
        let mut chart = String::from("graph TD;\n");
        let _ = writeln!(chart, "  VM[Micro-VM];");
        let _ = writeln!(
            chart,
            "  VM --> Kernel[\"{}\"];",
            self.kernel_path.display()
        );
        if let Some(initramfs) = &self.initramfs_path {
            let _ = writeln!(chart, "  VM --> Initramfs[\"{}\"];", initramfs.display());
        }
        if let Some(disk) = &self.disk_path {
            let _ = writeln!(chart, "  VM --> Disk[\"{}\"];", disk.display());
        }
        let _ = writeln!(chart, "  VM --> RAM[{} MiB RAM];", self.ram_mib);
        let _ = writeln!(chart, "  VM --> CPU[{} CPUs];", self.cpus);
        chart
    }
}

impl ToMermaid for MetricsSnapshot {
    fn to_mermaid(&self) -> String {
        let mut chart = String::from("pie title Resource Usage\n");
        let _ = writeln!(chart, "  \"CPU Used\" : {}", self.cpu.total_pct);
        let _ = writeln!(chart, "  \"CPU Free\" : {}", 100.0 - self.cpu.total_pct);
        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_to_mermaid() {
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

        let chart = config.to_mermaid();
        assert!(chart.contains("graph TD;"));
        assert!(chart.contains("VM --> Kernel[\"/boot/vmlinux\"];"));
        assert!(chart.contains("VM --> RAM[1024 MiB RAM];"));
        assert!(chart.contains("VM --> CPU[4 CPUs];"));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_metrics_to_mermaid() {
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 42.5,
                per_core: vec![],
                load_avg: [0.0, 0.0, 0.0],
            },
            memory: MemoryMetrics {
                total_bytes: 0,
                used_bytes: 0,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let chart = snap.to_mermaid();
        assert!(chart.contains("pie title Resource Usage"));
        assert!(chart.contains("\"CPU Used\" : 42.5"));
        assert!(chart.contains("\"CPU Free\" : 57.5"));
    }
}
