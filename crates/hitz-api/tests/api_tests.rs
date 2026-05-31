#![allow(missing_docs)]

use hitz_api::VmAction;

#[test]
fn test_vm_action_display_and_forms() {
    assert_eq!(VmAction::Start.to_string(), "start");
    assert_eq!(VmAction::Stop.to_string(), "stop");
    assert_eq!(VmAction::Restart.to_string(), "restart");

    assert_eq!(VmAction::Start.gerund(), "Starting");
    assert_eq!(VmAction::Stop.gerund(), "Stopping");
    assert_eq!(VmAction::Restart.gerund(), "Restarting");

    assert_eq!(VmAction::Start.past_tense(), "started");
    assert_eq!(VmAction::Stop.past_tense(), "stopped");
    assert_eq!(VmAction::Restart.past_tense(), "restarted");
}

#[cfg(feature = "terraform")]
mod terraform_tests {
    use hitz_api::{GuestAgentMode, ToTerraform, VmConfig};
    use std::path::PathBuf;

    #[test]
    fn test_to_terraform_full() {
        let config = VmConfig {
            kernel_path: PathBuf::from("/boot/vmlinux"),
            initramfs_path: Some(PathBuf::from("/boot/initrd.img")),
            disk_path: Some(PathBuf::from("/images/disk.img")),
            ram_mib: 512,
            cpus: 2,
            cmdline: Some("console=ttyS0".to_string()),
            net: None,
            ports: vec![],
            guest_cid: 3,
            guest_agent: GuestAgentMode::Auto,
        };

        let hcl = config.to_terraform("full_vm");
        assert!(hcl.contains("resource \"hitz_vm\" \"full_vm\" {"));
        assert!(hcl.contains("kernel_path = \"/boot/vmlinux\""));
        assert!(hcl.contains("initramfs_path = \"/boot/initrd.img\""));
        assert!(hcl.contains("disk_path = \"/images/disk.img\""));
        assert!(hcl.contains("ram_mib = 512"));
        assert!(hcl.contains("cpus = 2"));
        assert!(hcl.contains("cmdline = \"console=ttyS0\""));
        assert!(hcl.contains("guest_cid = 3"));
    }
}

#[cfg(feature = "imbalance")]
#[allow(clippy::float_cmp)]
mod imbalance_tests {
    use hitz_api::{CoreImbalanceAnalyzer, CpuMetrics, MemoryMetrics, MetricsSnapshot};

    #[test]
    fn test_imbalance_single_core_max() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 100.0,
                per_core: vec![100.0],
                load_avg: [1.0, 1.0, 1.0],
            },
            memory: MemoryMetrics {
                total_bytes: 1000,
                used_bytes: 500,
                free_bytes: 500,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let result = snap.analyze_imbalance();
        assert_eq!(result.std_dev, 0.0);
        assert_eq!(result.imbalance_score, 0.0);
        assert!(!result.is_imbalanced);
    }
}
