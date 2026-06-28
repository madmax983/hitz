with open('crates/hitz-api/src/rightsizer.rs', 'r') as f:
    content = f.read()

correct_header = """//! VM Rightsizing Recommendations.
//!
//! # Abstract
//! This module analyzes real-time telemetry (`MetricsSnapshot`) against the
//! current configuration (`VmConfig`) to provide actionable recommendations
//! for scaling resources up or down, optimizing for both performance and cost.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{RightSizer, ResizeRecommendation, VmConfig, MetricsSnapshot, CpuMetrics, MemoryMetrics, GuestAgentMode};
//! use std::path::PathBuf;
//!
//! // Assuming we have a heavily underutilized VM
//! let config = VmConfig {
//!     kernel_path: PathBuf::from("vmlinux"),
//!     initramfs_path: None,
//!     disk_path: None,
//!     ram_mib: 1024,
//!     cpus: 4,
//!     cmdline: None,
//!     net: None,
//!     ports: vec![],
//!     guest_cid: 3,
//!     guest_agent: GuestAgentMode::Auto,
//! };
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 5.0,
//!         per_core: vec![5.0, 5.0, 5.0, 5.0],
//!         load_avg: [0.1, 0.1, 0.1],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024 * 1024 * 1024,
//!         used_bytes: 100 * 1024 * 1024,
//!         free_bytes: 900 * 1024 * 1024,
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
//! let recommendations = snap.recommend_sizing(&config);
//! assert!(recommendations.len() > 0);
//! ```

use crate::{MetricsSnapshot, VmConfig};
use serde::{Deserialize, Serialize};

/// Specific actionable recommendation for resizing a VM."""

idx = content.find("/// Specific actionable recommendation for resizing a VM.")
if idx != -1:
    new_content = correct_header + content[idx + len("/// Specific actionable recommendation for resizing a VM."):]
    with open('crates/hitz-api/src/rightsizer.rs', 'w') as f:
        f.write(new_content)
