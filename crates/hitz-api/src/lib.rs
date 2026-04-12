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

/// Default kernel command line for Linux direct boot.
pub const DEFAULT_CMDLINE: &str = "console=ttyS0 earlyprintk=serial";

/// Default guest RAM in MiB (256 MiB).
pub const DEFAULT_RAM_MIB: u32 = 256;

/// Default number of virtual CPUs.
pub const DEFAULT_CPUS: u32 = 1;

/// Default host IP for guest networking.
pub const DEFAULT_HOST_IP: &str = "192.168.100.1/24";
/// Default guest IP for guest networking.
pub const DEFAULT_GUEST_IP: &str = "192.168.100.2/24";

/// Default guest CID for virtio-vsock (host=2, first guest=3).
pub const DEFAULT_GUEST_CID: u32 = 3;

/// Metrics port on which the guest agent listens and the host connects.
pub const VSOCK_METRICS_PORT: u32 = 52355;

/// Host CID as defined by the virtio-vsock spec.
pub const VMADDR_CID_HOST: u32 = 2;

/// API requests and responses
pub(crate) mod api;
/// VM Configuration types
pub(crate) mod config;
/// Guest agent related types
pub(crate) mod guest;
/// VM Lifecycle types
pub(crate) mod lifecycle;
/// VM metrics snapshot
pub(crate) mod metrics;
/// Network configuration types
pub(crate) mod net;

pub use crate::api::*;
pub use crate::config::*;
pub use crate::guest::*;
pub use crate::lifecycle::*;
pub use crate::metrics::*;
pub use crate::net::*;

#[cfg(feature = "health_check")]
/// Health assessment module for evaluating system telemetry.
mod health;
#[cfg(feature = "health_check")]
pub use health::*;

#[cfg(feature = "simulator")]
/// Simulator module for generating synthetic telemetry streams.
mod simulator;
#[cfg(feature = "simulator")]
pub use simulator::*;

#[cfg(feature = "diff")]
/// Diff module for calculating rates of change between telemetry snapshots.
mod diff;
#[cfg(feature = "diff")]
pub use diff::*;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_msgpack_roundtrip() {
        let snap = MetricsSnapshot {
            timestamp_ms: 123456789,
            cpu: CpuMetrics {
                total_pct: 10.5,
                per_core: vec![5.0, 15.0],
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
            disks: vec![],
            networks: vec![],
            processes: vec![],
        };

        let encoded = rmp_serde::to_vec(&snap).expect("encode");
        let decoded: MetricsSnapshot = rmp_serde::from_slice(&encoded).expect("decode");

        assert_eq!(decoded.timestamp_ms, snap.timestamp_ms);
        assert!((decoded.cpu.total_pct - snap.cpu.total_pct).abs() < f32::EPSILON);
    }

    #[test]
    fn guest_agent_mode_default_is_auto() {
        assert_eq!(GuestAgentMode::default(), GuestAgentMode::Auto);
    }
}
