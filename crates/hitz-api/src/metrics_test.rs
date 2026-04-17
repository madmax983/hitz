use super::*;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_and_deserialize_metrics_request() {
        let req = MetricsRequest::Snapshot;
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: MetricsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_cpu_metrics_default_fields() {
        let cpu = CpuMetrics {
            total_pct: 0.0,
            per_core: vec![],
            load_avg: [0.0, 0.0, 0.0],
        };
        let json = serde_json::to_string(&cpu).unwrap();
        let decoded: CpuMetrics = serde_json::from_str(&json).unwrap();
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
        let json = serde_json::to_string(&mem).unwrap();
        let decoded: MemoryMetrics = serde_json::from_str(&json).unwrap();
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
        let json = serde_json::to_string(&disk).unwrap();
        let decoded: DiskMetrics = serde_json::from_str(&json).unwrap();
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
        let json = serde_json::to_string(&net).unwrap();
        let decoded: NetMetrics = serde_json::from_str(&json).unwrap();
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
        let json = serde_json::to_string(&proc).unwrap();
        let decoded: ProcMetrics = serde_json::from_str(&json).unwrap();
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
        let encoded_msgpack = rmp_serde::to_vec(&snap).unwrap();
        let decoded_msgpack: MetricsSnapshot = rmp_serde::from_slice(&encoded_msgpack).unwrap();
        assert_eq!(snap, decoded_msgpack);

        // JSON roundtrip
        let encoded_json = serde_json::to_string(&snap).unwrap();
        let decoded_json: MetricsSnapshot = serde_json::from_str(&encoded_json).unwrap();
        assert_eq!(snap, decoded_json);
    }
}
