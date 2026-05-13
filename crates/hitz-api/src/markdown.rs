//! Markdown exporter module.
//!
//! # Abstract
//! Converts API data structures into formatted Markdown and Mermaid.js diagrams.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ToMarkdown, VmConfig, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 512,
//!     cpus: 2,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Disabled,
//! };
//!
//! let markdown = config.to_markdown();
//! assert!(markdown.contains("```mermaid"));
//! ```

use crate::{MetricsSnapshot, VmConfig};
use std::fmt::Write;

/// Trait for objects that can be exported to Markdown format.
pub trait ToMarkdown {
    /// Generates a Markdown representation of the object.
    fn to_markdown(&self) -> String;
}

impl ToMarkdown for VmConfig {
    fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("## VM Configuration\n\n");
        let _ = writeln!(md, "- **CPUs**: {}", self.cpus);
        let _ = writeln!(md, "- **RAM**: {} MiB", self.ram_mib);
        let _ = writeln!(md, "- **Kernel**: `{}`", self.kernel_path.display());

        if let Some(ref initramfs) = self.initramfs_path {
            let _ = writeln!(md, "- **Initramfs**: `{}`", initramfs.display());
        }
        if let Some(ref disk) = self.disk_path {
            let _ = writeln!(md, "- **Disk**: `{}`", disk.display());
        }

        md.push_str("\n### Architecture Diagram\n\n");
        md.push_str("```mermaid\n");
        md.push_str("graph TD\n");
        md.push_str("  Host[Host System] -->|virtio-vsock| VM[Micro-VM]\n");
        let _ = writeln!(md, "  VM --> CPU[{} vCPUs]", self.cpus);
        let _ = writeln!(md, "  VM --> RAM[{} MiB Memory]", self.ram_mib);

        if let Some(ref net) = self.net {
            let _ = writeln!(md, "  Host -->|{}| VM", net.host_ip);
        }
        md.push_str("```\n");

        md
    }
}

impl ToMarkdown for MetricsSnapshot {
    fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("## Metrics Snapshot\n\n");
        let _ = writeln!(md, "**Timestamp**: {}\n", self.timestamp_ms);

        md.push_str("### CPU\n");
        let _ = writeln!(md, "**Total Usage**: {:.1}%\n", self.cpu.total_pct);
        md.push_str("| Core | Usage |\n");
        md.push_str("|---|---|\n");
        for (i, pct) in self.cpu.per_core.iter().enumerate() {
            let _ = writeln!(md, "| Core {i} | {pct:.1}% |");
        }
        md.push('\n');

        md.push_str("### Memory\n");
        md.push_str("| Metric | Bytes |\n");
        md.push_str("|---|---|\n");
        let _ = writeln!(md, "| Total | {} |", self.memory.total_bytes);
        let _ = writeln!(md, "| Used | {} |", self.memory.used_bytes);
        let _ = writeln!(md, "| Free | {} |", self.memory.free_bytes);
        md.push('\n');

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, GuestAgentMode, MemoryMetrics};
    use std::path::PathBuf;

    #[test]
    fn test_vmconfig_markdown() {
        let config = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: 1024,
            cpus: 4,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Disabled,
        };
        let md = config.to_markdown();
        assert!(md.contains("## VM Configuration"));
        assert!(md.contains("4 vCPUs"));
        assert!(md.contains("1024 MiB Memory"));
        assert!(md.contains("```mermaid"));
    }

    #[test]
    fn test_metrics_snapshot_markdown() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 25.0,
                per_core: vec![50.0, 0.0],
                load_avg: [0.1, 0.1, 0.1],
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
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let md = snap.to_markdown();
        assert!(md.contains("## Metrics Snapshot"));
        assert!(md.contains("25.0%"));
        assert!(md.contains("| Core 0 | 50.0% |"));
        assert!(md.contains("| Total | 1024 |"));
    }
}
