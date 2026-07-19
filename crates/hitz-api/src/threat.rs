//! Threat detection module for evaluating suspicious VM behavior.
//!
//! # Abstract
//! Analyzes resource utilization patterns to detect potential security threats
//! such as cryptomining, ransomware activity, or network scanning.
//!
//! # The Hero's Journey
//!
//! ```rust
//! use hitz_api::{ThreatDetector, ThreatType, MetricsSnapshot, CpuMetrics, MemoryMetrics};
//!
//! let snap = MetricsSnapshot {
//!     timestamp_ms: 1000,
//!     cpu: CpuMetrics {
//!         total_pct: 99.0,
//!         per_core: vec![99.0, 99.0],
//!         load_avg: [2.0, 2.0, 2.0],
//!     },
//!     memory: MemoryMetrics {
//!         total_bytes: 1024,
//!         used_bytes: 512,
//!         free_bytes: 512,
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
//! let assessment = snap.analyze_threats();
//! assert!(assessment.is_threat);
//! assert!(assessment.threats.contains(&ThreatType::CryptoMining));
//! ```

use crate::MetricsSnapshot;
use serde::{Deserialize, Serialize};

/// Types of detected threats based on resource heuristics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatType {
    /// Sustained near-100% CPU usage with minimal I/O (typical of cryptominers).
    CryptoMining,
    /// Unusually high disk write activity compared to reads (typical of ransomware).
    RansomwareBehavior,
    /// Excessive network packets with high error rates (typical of port scanning/flooding).
    NetworkScanning,
    /// Memory exhaustion paired with high swap usage (typical of `DoS` or fork bombs).
    DenialOfService,
}

/// The result of a threat analysis heuristic pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreatAssessment {
    /// Whether any significant threats were detected.
    pub is_threat: bool,
    /// An overall risk score (0.0 to 100.0).
    pub risk_score: f64,
    /// List of detected threat profiles.
    pub threats: Vec<ThreatType>,
    /// Human-readable reasons for the assessment.
    pub insights: Vec<String>,
}

/// Trait to analyze metrics for potential security threats.
pub trait ThreatDetector {
    /// Evaluates the metrics snapshot and returns a threat assessment.
    fn analyze_threats(&self) -> ThreatAssessment;
}

impl ThreatDetector for MetricsSnapshot {
    #[allow(clippy::cast_precision_loss)]
    fn analyze_threats(&self) -> ThreatAssessment {
        let mut threats = Vec::new();
        let mut insights = Vec::new();
        let mut risk_score: f64 = 0.0;

        // 1. CryptoMining Heuristic
        // High CPU (near 100%), but low disk and network activity.
        if self.cpu.total_pct > 95.0 {
            let total_disk_io: u64 = self
                .disks
                .iter()
                .map(|d| d.reads_total + d.writes_total)
                .sum();
            let total_net_io: u64 = self
                .networks
                .iter()
                .map(|n| n.rx_packets + n.tx_packets)
                .sum();

            if total_disk_io < 1000 && total_net_io < 5000 {
                threats.push(ThreatType::CryptoMining);
                insights.push(
                    "Suspiciously high CPU usage with minimal I/O (CryptoMining profile)."
                        .to_string(),
                );
                risk_score += 60.0;
            }
        }

        // 2. Ransomware Heuristic
        // Extremely high disk writes compared to reads
        let total_reads: u64 = self.disks.iter().map(|d| d.reads_total).sum();
        let total_writes: u64 = self.disks.iter().map(|d| d.writes_total).sum();
        if total_writes > 10_000 && total_writes > (total_reads * 10) {
            threats.push(ThreatType::RansomwareBehavior);
            insights.push(
                "Abnormal disk write-to-read ratio detected (Ransomware profile).".to_string(),
            );
            risk_score += 80.0;
        }

        // 3. Network Scanning Heuristic
        // High tx packets with high errors
        let total_tx_packets: u64 = self.networks.iter().map(|n| n.tx_packets).sum();
        let total_tx_errors: u64 = self.networks.iter().map(|n| n.tx_errors).sum();

        if total_tx_packets > 10_000 && total_tx_errors > 500 {
            threats.push(ThreatType::NetworkScanning);
            insights.push(
                "High volume of network transmit errors (Port Scanning/Flood profile).".to_string(),
            );
            risk_score += 50.0;
        }

        // 4. Denial of Service (Memory)
        if self.memory.total_bytes > 0 {
            let mem_usage_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            let swap_usage_pct = if self.memory.swap_total > 0 {
                (self.memory.swap_used as f64 / self.memory.swap_total as f64) * 100.0
            } else {
                0.0
            };

            if mem_usage_pct > 95.0 && swap_usage_pct > 80.0 {
                threats.push(ThreatType::DenialOfService);
                insights
                    .push("Memory and swap nearly exhausted (DoS/Fork bomb profile).".to_string());
                risk_score += 70.0;
            }
        }

        ThreatAssessment {
            is_threat: risk_score >= 40.0,
            risk_score: risk_score.clamp(0.0, 100.0),
            threats,
            insights,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{CpuMetrics, DiskMetrics, MemoryMetrics, NetMetrics};

    fn safe_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp_ms: 1000,
            cpu: CpuMetrics {
                total_pct: 10.0,
                per_core: vec![10.0],
                load_avg: [0.1, 0.1, 0.1],
            },
            memory: MemoryMetrics {
                total_bytes: 1024 * 1024,
                used_bytes: 512 * 1024,
                free_bytes: 512 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 1024 * 1024,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".to_string(),
                read_bytes: 1024,
                write_bytes: 1024,
                reads_total: 100,
                writes_total: 100,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".to_string(),
                rx_bytes: 1024,
                tx_bytes: 1024,
                rx_packets: 100,
                tx_packets: 100,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        }
    }

    #[test]
    fn test_no_threats() {
        let snap = safe_snapshot();
        let assessment = snap.analyze_threats();
        assert!(!assessment.is_threat);
        assert_eq!(assessment.risk_score, 0.0);
        assert!(assessment.threats.is_empty());
    }

    #[test]
    fn test_cryptomining_threat() {
        let mut snap = safe_snapshot();
        snap.cpu.total_pct = 99.0;
        // Keep IO low
        snap.disks[0].reads_total = 0;
        snap.disks[0].writes_total = 0;

        let assessment = snap.analyze_threats();
        assert!(assessment.is_threat);
        assert!(assessment.threats.contains(&ThreatType::CryptoMining));
        assert!(assessment.risk_score >= 60.0);
    }

    #[test]
    fn test_ransomware_threat() {
        let mut snap = safe_snapshot();
        snap.disks[0].reads_total = 100;
        snap.disks[0].writes_total = 15_000; // > 10,000 and > 10 * reads

        let assessment = snap.analyze_threats();
        assert!(assessment.is_threat);
        assert!(assessment.threats.contains(&ThreatType::RansomwareBehavior));
    }

    #[test]
    fn test_network_scanning_threat() {
        let mut snap = safe_snapshot();
        snap.networks[0].tx_packets = 15_000;
        snap.networks[0].tx_errors = 600;

        let assessment = snap.analyze_threats();
        assert!(assessment.is_threat);
        assert!(assessment.threats.contains(&ThreatType::NetworkScanning));
    }

    #[test]
    fn test_dos_threat() {
        let mut snap = safe_snapshot();
        snap.memory.used_bytes = (1024.0 * 1024.0 * 0.96) as u64; // > 95%
        snap.memory.swap_used = (1024.0 * 1024.0 * 0.85) as u64; // > 80%

        let assessment = snap.analyze_threats();
        assert!(assessment.is_threat);
        assert!(assessment.threats.contains(&ThreatType::DenialOfService));
    }

    #[test]
    fn test_multiple_threats_capped_score() {
        let mut snap = safe_snapshot();
        // Trigger CryptoMining
        snap.cpu.total_pct = 99.0;
        snap.disks[0].reads_total = 0;
        snap.disks[0].writes_total = 0;

        // Trigger Network Scanning (wait, this would raise total IO and suppress cryptomining? Let's check.)
        // total_net_io is rx_packets + tx_packets. If tx_packets = 15_000, total_net_io is > 5000, so CryptoMining won't trigger.
        // Let's trigger DoS and Ransomware instead.
        snap.memory.used_bytes = (1024.0 * 1024.0 * 0.96) as u64;
        snap.memory.swap_used = (1024.0 * 1024.0 * 0.85) as u64;

        snap.disks[0].reads_total = 100;
        snap.disks[0].writes_total = 15_000;

        let assessment = snap.analyze_threats();
        assert!(assessment.is_threat);
        assert!(assessment.threats.contains(&ThreatType::DenialOfService));
        assert!(assessment.threats.contains(&ThreatType::RansomwareBehavior));
        assert_eq!(assessment.risk_score, 100.0); // Capped at 100.0
    }
}
