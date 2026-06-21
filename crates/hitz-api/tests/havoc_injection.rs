#![allow(missing_docs)]
use hitz_api::{VmConfig, GuestAgentMode, MetricsSnapshot, CpuMetrics, MemoryMetrics};
#[cfg(feature = "terraform")]
use hitz_api::ToTerraform;
#[cfg(feature = "prometheus")]
use hitz_api::ToPrometheus;

#[test]
#[cfg(feature = "terraform")]
fn test_terraform_injection() {
    let config = VmConfig {
        kernel_path: std::path::PathBuf::from("/boot/vmlinux"),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 512,
        cpus: 2,
        cmdline: None,
        net: None,
        ports: vec![],
        guest_cid: 3,
        guest_agent: GuestAgentMode::Auto,
    };

    let hcl = config.to_terraform("foo\"\nresource \"evil\" \"hack\" {\n");
    assert!(!hcl.contains("resource \"evil\" \"hack\" {"), "Assertion failed: Terraform injection vulnerability");
}

#[test]
#[cfg(feature = "prometheus")]
fn test_prometheus_injection() {
    let snap = MetricsSnapshot {
        timestamp_ms: 1_700_000_000_000,
        cpu: CpuMetrics {
            total_pct: 0.0,
            per_core: vec![],
            load_avg: [0.0; 3],
        },
        memory: MemoryMetrics {
            total_bytes: 0,
            free_bytes: 0,
            buffers_bytes: 0,
            cached_bytes: 0,
            used_bytes: 0,
            swap_total: 0,
            swap_used: 0,
        },
        disks: vec![],
        networks: vec![],
        processes: vec![],
    };

    let prom = snap.to_prometheus("foo\"\nhitz_cpu_total_pct{vm_id=\"bar\"} 100 1700000000000\n\"");
    assert!(!prom.contains("hitz_cpu_total_pct{vm_id=\"bar\"} 100 1700000000000"), "Assertion failed: Prometheus injection vulnerability");
}
