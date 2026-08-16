//! REST API types for Hitz.
//!
//! This crate serves as the central vocabulary for the Hitz micro-VM manager.
//! It defines the pure data structures used for communicating between the
//! command-line interface, the background daemon, and the guest agents running
//! inside the micro-VMs.
//!
//! By keeping all I/O out of this crate, we ensure these types can be compiled
//! for any target (including the Linux `musl` guest agents) and serialized
//! effortlessly across network boundaries.
//!
//! ## Core Concepts
//!
//! * **Configuration**: [`VmConfig`] describes everything needed to boot a VM.
//! * **Lifecycle**: [`VmState`] and [`VmAction`] track and control the VM's status.
//! * **Metrics**: [`MetricsSnapshot`] and its sub-types (like [`CpuMetrics`]) form the
//!   wire protocol over vsock for extracting real-time telemetry from the guest.

/// API structures for interacting with the background daemon.
mod api;
/// Configuration structures representing the VM specification.
mod config;
/// Telemetry structures representing the VM's runtime resources.
mod metrics;

pub use api::{
    ActionVmRequest, ApiError, CloneVmRequest, CreateVmRequest, VmAction, VmInfo, VmState,
};
pub use config::{
    DEFAULT_CMDLINE, DEFAULT_CPUS, DEFAULT_GUEST_CID, DEFAULT_GUEST_IP, DEFAULT_HOST_IP,
    DEFAULT_RAM_MIB, GuestAgentMode, NetConfig, PortForward, VMADDR_CID_HOST, VSOCK_METRICS_PORT,
    VmConfig,
};
pub use metrics::{
    CpuMetrics, DiskMetrics, MemoryMetrics, MetricsRequest, MetricsSnapshot, NetMetrics,
    ProcMetrics,
};

#[cfg(feature = "health_check")]
/// Health assessment module for evaluating system telemetry.
mod health;
#[cfg(feature = "health_check")]
pub use health::{HealthCheck, HealthStatus, SystemHealth};

#[cfg(feature = "simulator")]
/// Simulator module for generating synthetic telemetry streams.
mod simulator;
#[cfg(feature = "simulator")]
pub use simulator::{VmSimulator, WorkloadProfile};

#[cfg(feature = "diff")]
/// Diff module for calculating rates of change between telemetry snapshots.
mod diff;
#[cfg(feature = "diff")]
pub use diff::{CalculateDiff, DiskRate, MetricsDiff, NetRate};

#[cfg(feature = "classifier")]
/// Classifier module for determining workload type.
mod classifier;
#[cfg(feature = "classifier")]
pub use classifier::{WorkloadClass, WorkloadClassifier};

#[cfg(feature = "prometheus")]
/// Prometheus module for converting metrics to Prometheus text format.
mod prometheus;
#[cfg(feature = "prometheus")]
pub use prometheus::ToPrometheus;

#[cfg(feature = "carbon")]
/// Carbon footprint estimation module.
mod carbon;
#[cfg(feature = "carbon")]
pub use carbon::{CarbonEstimator, EmissionFactors};

#[cfg(feature = "sentinel")]
/// Sentinel module for defining rules based on metrics.
mod sentinel;
#[cfg(feature = "sentinel")]
pub use sentinel::{ConditionOperator, MetricTarget, SentinelCondition, SentinelRule};

#[cfg(test)]
#[allow(clippy::expect_used)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Helper to build a minimal VmConfig ───────────────────────────────────

    fn minimal_config() -> VmConfig {
        VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: None,
            disk_path: None,
            ram_mib: DEFAULT_RAM_MIB,
            cpus: 1,
            cmdline: None,
            net: None,
            ports: vec![],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        }
    }

    // ── Phase 12 new tests ────────────────────────────────────────────────────

    #[test]
    fn metrics_snapshot_msgpack_roundtrip() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1_700_000_000_000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 156 * 1024 * 1024,
                buffers_bytes: 10 * 1024 * 1024,
                cached_bytes: 30 * 1024 * 1024,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };
        let encoded = rmp_serde::to_vec(&snap).expect("encode");
        let decoded: MetricsSnapshot = rmp_serde::from_slice(&encoded).expect("decode");
        assert!((decoded.cpu.total_pct - 12.5).abs() < f32::EPSILON);
        assert_eq!(decoded.memory.total_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn guest_agent_mode_default_is_auto() {
        let mode: GuestAgentMode = GuestAgentMode::default();
        assert!(matches!(mode, GuestAgentMode::Auto));
    }

    #[test]
    fn guest_agent_mode_custom_serde_roundtrip() {
        let mode = GuestAgentMode::Custom(std::path::PathBuf::from("/usr/local/bin/my-agent"));
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }

    #[test]
    fn guest_agent_mode_disabled_serde_roundtrip() {
        let mode = GuestAgentMode::Disabled;
        let json = serde_json::to_string(&mode).expect("serialize");
        let decoded: GuestAgentMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, mode);
    }

    #[test]
    fn default_config_has_correct_values() {
        let json = r#"{"kernel_path": "vmlinux", "ram_mib": 256}"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.guest_agent, GuestAgentMode::Auto);
        assert_eq!(cfg.guest_cid, DEFAULT_GUEST_CID);
        assert_eq!(cfg.cpus, DEFAULT_CPUS);
    }

    // ── Existing tests (updated for new VmConfig fields) ─────────────────────

    #[test]
    fn test_port_forward_serde() {
        let pf = PortForward {
            host_port: 8080,
            guest_port: 80,
        };
        let json = serde_json::to_string(&pf).expect("serialize");
        assert_eq!(json, r#"{"host_port":8080,"guest_port":80}"#);
    }

    #[test]
    fn test_vmconfig_serde() {
        let cfg = VmConfig {
            kernel_path: PathBuf::from("vmlinux"),
            initramfs_path: Some(PathBuf::from("initrd")),
            disk_path: None,
            ram_mib: 512,
            cpus: 2,
            cmdline: Some("quiet".to_string()),
            net: Some(NetConfig {
                mac: None,
                host_ip: "10.0.0.1/24".to_string(),
                guest_ip: "10.0.0.2/24".to_string(),
                adapter_name: None,
            }),
            ports: vec![PortForward {
                host_port: 2222,
                guest_port: 22,
            }],
            guest_cid: DEFAULT_GUEST_CID,
            guest_agent: GuestAgentMode::Auto,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, restored);
    }

    #[test]
    fn test_create_vm_request_serde() {
        let req = CreateVmRequest {
            config: minimal_config(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CreateVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req.config, restored.config);
    }

    #[test]
    fn test_action_vm_request_serde() {
        let req = ActionVmRequest {
            action: VmAction::Start,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: ActionVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req.action, restored.action);
    }

    #[test]
    fn test_vminfo_serde() {
        let info = VmInfo {
            id: "vm-1".to_string(),
            state: VmState::Running,
            config: minimal_config(),
            exit_reason: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: VmInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(info.id, restored.id);
        assert_eq!(info.state, restored.state);
        assert_eq!(info.config, restored.config);
    }

    #[test]
    fn test_apierror_serde() {
        let err = ApiError {
            message: "Not found".to_string(),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let restored: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err.message, restored.message);
    }

    #[test]
    fn test_clone_vm_request_serde() {
        let req = CloneVmRequest {
            dest_id: "vm-2".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CloneVmRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req.dest_id, restored.dest_id);
    }

    #[test]
    fn test_vmconfig_effective_cmdline() {
        let mut cfg = minimal_config();
        assert_eq!(cfg.effective_cmdline(), DEFAULT_CMDLINE);
        cfg.cmdline = Some("root=/dev/vda".to_string());
        assert_eq!(cfg.effective_cmdline(), "root=/dev/vda");
    }

    #[test]
    fn test_vmconfig_default_ports() {
        let json = r#"{
            "kernel_path": "vmlinux",
            "ram_mib": 256,
            "cpus": 1
        }"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert!(cfg.ports.is_empty());
    }

    #[test]
    fn test_vmconfig_with_ports() {
        let cfg = VmConfig {
            ports: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
            }],
            ..minimal_config()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.ports.len(), 1);
        assert_eq!(restored.ports[0].host_port, 8080);
    }

    #[test]
    fn test_vmconfig_default_net() {
        let json = r#"{
            "kernel_path": "vmlinux",
            "ram_mib": 256,
            "cpus": 1
        }"#;
        let cfg: VmConfig = serde_json::from_str(json).expect("deserialize");
        assert!(cfg.net.is_none());
    }

    #[test]
    fn test_vmconfig_with_net() {
        let cfg = VmConfig {
            net: Some(NetConfig {
                mac: Some("AA:BB:CC:DD:EE:FF".to_string()),
                host_ip: "10.0.0.1/24".to_string(),
                guest_ip: "10.0.0.2/24".to_string(),
                adapter_name: Some("hitz-test".to_string()),
            }),
            ..minimal_config()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: VmConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.net.is_some());
        assert_eq!(
            restored.net.expect("net").mac.expect("mac"),
            "AA:BB:CC:DD:EE:FF"
        );
    }

    #[test]
    fn should_serialize_and_deserialize_metrics_request() {
        let req = MetricsRequest::Snapshot;
        let json = serde_json::to_string(&req).expect("serialize/deserialize failed");
        let deserialized: MetricsRequest =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_cpu_metrics_default_fields() {
        let cpu = CpuMetrics {
            total_pct: 0.0,
            per_core: vec![],
            load_avg: [0.0, 0.0, 0.0],
        };
        let json = serde_json::to_string(&cpu).expect("serialize/deserialize failed");
        let decoded: CpuMetrics =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(cpu, decoded);
    }

    #[test]
    fn test_memory_metrics_zero_values() {
        let mem = MemoryMetrics {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            buffers_bytes: 0,
            cached_bytes: 0,
            swap_total: 0,
            swap_used: 0,
        };
        let json = serde_json::to_string(&mem).expect("serialize/deserialize failed");
        let decoded: MemoryMetrics =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(mem, decoded);
    }

    #[test]
    fn test_disk_metrics_serialization() {
        let disk = DiskMetrics {
            name: "sda".to_string(),
            read_bytes: 1024,
            write_bytes: 2048,
            reads_total: 10,
            writes_total: 20,
        };
        let json = serde_json::to_string(&disk).expect("serialize/deserialize failed");
        let decoded: DiskMetrics =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(disk, decoded);
    }

    #[test]
    fn test_net_metrics_serialization() {
        let net = NetMetrics {
            interface: "eth0".to_string(),
            rx_bytes: 5000,
            tx_bytes: 1000,
            rx_packets: 50,
            tx_packets: 10,
            rx_errors: 0,
            tx_errors: 0,
        };
        let json = serde_json::to_string(&net).expect("serialize/deserialize failed");
        let decoded: NetMetrics =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(net, decoded);
    }

    #[test]
    fn test_proc_metrics_serialization() {
        let proc = ProcMetrics {
            pid: 1,
            name: "systemd".to_string(),
            cpu_pct: 0.1,
            rss_bytes: 4096,
            state: 'S',
        };
        let json = serde_json::to_string(&proc).expect("serialize/deserialize failed");
        let decoded: ProcMetrics =
            serde_json::from_str(&json).expect("serialize/deserialize failed");
        assert_eq!(proc, decoded);
    }

    #[test]
    fn test_metrics_snapshot_roundtrip() {
        let snap = MetricsSnapshot {
            timestamp_ms: 1_680_000_000_000,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.5, 0.4, 0.3],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 100 * 1024 * 1024,
                free_bytes: 156 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".to_string(),
                read_bytes: 1024,
                write_bytes: 2048,
                reads_total: 10,
                writes_total: 20,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 5000,
                tx_bytes: 1000,
                rx_packets: 50,
                tx_packets: 10,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![ProcMetrics {
                pid: 1,
                name: "systemd".to_string(),
                cpu_pct: 0.1,
                rss_bytes: 4096,
                state: 'S',
            }],
        };

        // MsgPack roundtrip
        let encoded_msgpack = rmp_serde::to_vec(&snap).expect("serialize/deserialize failed");
        let decoded_msgpack: MetricsSnapshot =
            rmp_serde::from_slice(&encoded_msgpack).expect("serialize/deserialize failed");
        assert_eq!(snap, decoded_msgpack);

        // JSON roundtrip
        let encoded_json = serde_json::to_string(&snap).expect("serialize/deserialize failed");
        let decoded_json: MetricsSnapshot =
            serde_json::from_str(&encoded_json).expect("serialize/deserialize failed");
        assert_eq!(snap, decoded_json);
    }
}
#[cfg(feature = "efficiency")]
/// Efficiency scoring module for evaluating resource usage.
mod efficiency;
#[cfg(feature = "efficiency")]
pub use efficiency::{EfficiencyScore, EfficiencyScorer};

#[cfg(feature = "fingerprint")]
/// Fingerprinting module for categorizing VM workload behavior.
mod fingerprint;
#[cfg(feature = "fingerprint")]
pub use fingerprint::{FingerprintGenerator, VmFingerprint};

#[cfg(feature = "imbalance")]
/// Imbalance scoring module for evaluating per-core CPU utilization imbalance.
mod imbalance;
#[cfg(feature = "imbalance")]
pub use imbalance::{CoreImbalanceAnalyzer, ImbalanceResult};

#[cfg(feature = "rightsizer")]
/// Rightsizing module for analyzing metrics and suggesting config changes.
mod rightsizer;
#[cfg(feature = "rightsizer")]
pub use rightsizer::{ResizeRecommendation, RightSizer};

#[cfg(feature = "terraform")]
/// Terraform HCL generation module.
mod terraform;
#[cfg(feature = "terraform")]
pub use terraform::ToTerraform;
