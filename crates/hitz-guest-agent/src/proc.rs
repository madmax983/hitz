//! Parsers for Linux /proc virtual filesystem entries.

use hitz_api::{DiskMetrics, MemoryMetrics, NetMetrics};

/// Raw CPU tick sample from one `/proc/stat` line.
#[derive(Debug, Clone, Default)]
pub struct CpuSample {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
}

impl CpuSample {
    /// Total active (non-idle) ticks.
    pub const fn active(&self) -> u64 {
        self.user
            .saturating_add(self.nice)
            .saturating_add(self.system)
            .saturating_add(self.irq)
            .saturating_add(self.softirq)
    }

    /// Total ticks (active + idle).
    pub const fn total(&self) -> u64 {
        self.active()
            .saturating_add(self.idle)
            .saturating_add(self.iowait)
    }
}

/// Calculate CPU utilisation % between two samples.
#[must_use]
pub fn cpu_pct(prev: &CpuSample, curr: &CpuSample) -> f32 {
    let total_delta = curr.total().saturating_sub(prev.total());
    if total_delta == 0 {
        return 0.0;
    }
    let active_delta = curr.active().saturating_sub(prev.active());
    #[allow(clippy::cast_precision_loss)]
    {
        (active_delta as f32 / total_delta as f32) * 100.0
    }
}

/// Parse `/proc/stat` into per-CPU samples (index 0 = aggregate "cpu" line).
#[must_use]
pub fn parse_proc_stat_sample(content: &str) -> Vec<CpuSample> {
    content
        .lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|line| {
            let mut nums = line
                .split_ascii_whitespace()
                .skip(1)
                .map(|s| s.parse().unwrap_or(0));
            CpuSample {
                user: nums.next().unwrap_or(0),
                nice: nums.next().unwrap_or(0),
                system: nums.next().unwrap_or(0),
                idle: nums.next().unwrap_or(0),
                iowait: nums.next().unwrap_or(0),
                irq: nums.next().unwrap_or(0),
                softirq: nums.next().unwrap_or(0),
            }
        })
        .collect()
}

/// Parse `/proc/meminfo` into [`MemoryMetrics`].
///
/// Returns `None` if `MemTotal` is missing.
#[must_use]
pub fn parse_proc_meminfo(content: &str) -> Option<MemoryMetrics> {
    let mut total = 0u64;
    let mut free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in content.lines() {
        let mut parts = line.split_ascii_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(val_str) = parts.next() else {
            continue;
        };
        let Ok(val) = val_str.parse::<u64>() else {
            continue;
        };
        let val = val.saturating_mul(1024);
        match key {
            "MemTotal:" => total = val,
            "MemFree:" => free = val,
            "Buffers:" => buffers = val,
            "Cached:" => cached = val,
            "SwapTotal:" => swap_total = val,
            "SwapFree:" => swap_free = val,
            _ => {}
        }
    }

    if total == 0 {
        return None;
    }

    Some(MemoryMetrics {
        total_bytes: total,
        used_bytes: total.saturating_sub(free.saturating_add(buffers).saturating_add(cached)),
        free_bytes: free,
        buffers_bytes: buffers,
        cached_bytes: cached,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    })
}

/// Parse `/proc/diskstats` into a list of [`DiskMetrics`].
///
/// Only includes devices with entries present in the file.
#[must_use]
pub fn parse_proc_diskstats(content: &str) -> Vec<DiskMetrics> {
    content
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_ascii_whitespace();
            let name = cols.nth(2)?.to_string();
            let reads = cols.next()?.parse::<u64>().ok()?;
            let _reads_merged = cols.next()?;
            let read_sec = cols.next()?.parse::<u64>().ok()?;
            let _ms_reading = cols.next()?;
            let writes = cols.next()?.parse::<u64>().ok()?;
            let _writes_merged = cols.next()?;
            let write_sec = cols.next()?.parse::<u64>().ok()?;
            Some(DiskMetrics {
                name,
                reads_total: reads,
                writes_total: writes,
                read_bytes: read_sec.saturating_mul(512),
                write_bytes: write_sec.saturating_mul(512),
            })
        })
        .collect()
}

/// Parse `/proc/net/dev` into a list of [`NetMetrics`].
///
/// Skips the two header lines and the loopback interface.
#[must_use]
pub fn parse_proc_net_dev(content: &str) -> Vec<NetMetrics> {
    content
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (iface, stats) = line.split_once(':')?;
            let iface = iface.trim().to_string();
            if iface == "lo" {
                return None;
            }
            let mut cols = stats
                .split_ascii_whitespace()
                .map(|s| s.parse().unwrap_or(0));
            Some(NetMetrics {
                interface: iface,
                rx_bytes: cols.next().unwrap_or(0),
                rx_packets: cols.next().unwrap_or(0),
                rx_errors: cols.next().unwrap_or(0),
                tx_bytes: cols.nth(5).unwrap_or(0),
                tx_packets: cols.next().unwrap_or(0),
                tx_errors: cols.next().unwrap_or(0),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_STAT_FIXTURE: &str = "cpu  12345 678 9012 345678 123 0 456 0 0 0\n\
         cpu0 6000 300 4000 170000 60 0 200 0 0 0\n\
         cpu1 6345 378 5012 175678 63 0 256 0 0 0\n";

    const PROC_MEMINFO_FIXTURE: &str = "MemTotal:        262144 kB\n\
         MemFree:         131072 kB\n\
         Buffers:          10240 kB\n\
         Cached:           20480 kB\n\
         SwapTotal:            0 kB\n\
         SwapFree:             0 kB\n";

    const PROC_DISKSTATS_FIXTURE: &str = "   8   0 vda 100 0 800 20 50 0 400 10 0 30 30\n";

    const PROC_NET_DEV_FIXTURE: &str = "Inter-|   Receive                                                |  Transmit\n\
         face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
         eth0:    5000     40    0    0    0     0          0         0     2000      20    0    0    0     0       0          0\n";

    #[test]
    fn parse_cpu_stat() {
        let stats = parse_proc_stat_sample(PROC_STAT_FIXTURE);
        // Should have total + 2 cores.
        assert_eq!(stats.len(), 3);
        // Total idle ticks.
        assert_eq!(stats[0].idle, 345_678);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn parse_meminfo() {
        let mem = parse_proc_meminfo(PROC_MEMINFO_FIXTURE).expect("parse");
        assert_eq!(mem.total_bytes, 262_144 * 1024);
        assert_eq!(mem.free_bytes, 131_072 * 1024);
        assert_eq!(mem.swap_total, 0);
    }

    #[test]
    fn parse_diskstats() {
        let disks = parse_proc_diskstats(PROC_DISKSTATS_FIXTURE);
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].name, "vda");
        assert_eq!(disks[0].reads_total, 100);
        assert_eq!(disks[0].read_bytes, 800 * 512); // sectors × 512
    }

    #[test]
    fn parse_net_dev() {
        let nets = parse_proc_net_dev(PROC_NET_DEV_FIXTURE);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].interface, "eth0");
        assert_eq!(nets[0].rx_bytes, 5000);
        assert_eq!(nets[0].tx_bytes, 2000);
    }

    #[test]
    fn havoc_overflow_proc_stat() {
        // u64::MAX should not panic during parsing/summation
        let stats = parse_proc_stat_sample(
            "cpu 18446744073709551615 18446744073709551615 18446744073709551615 18446744073709551615 18446744073709551615 18446744073709551615 18446744073709551615",
        );
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].active(), u64::MAX);
        assert_eq!(stats[0].total(), u64::MAX);

        #[allow(clippy::expect_used)]
        let mem = parse_proc_meminfo(
            "MemTotal: 18446744073709551615 kB\nMemFree: 18446744073709551615 kB",
        )
        .expect("parse");
        assert_eq!(mem.total_bytes, u64::MAX);

        let disks = parse_proc_diskstats(
            "   8   0 vda 100 0 18446744073709551615 20 50 0 18446744073709551615 10 0 30 30\n",
        );
        assert_eq!(disks[0].read_bytes, u64::MAX);
        assert_eq!(disks[0].write_bytes, u64::MAX);
    }

    #[test]
    fn cpu_utilisation_from_two_samples() {
        // a: active=1200, total=10000
        // b: active=1450, total=11000  →  delta_active=250, delta_total=1000 → 25%
        let a = CpuSample {
            user: 1000,
            nice: 0,
            system: 200,
            idle: 8800,
            iowait: 0,
            irq: 0,
            softirq: 0,
        };
        let b = CpuSample {
            user: 1200,
            nice: 0,
            system: 250,
            idle: 9550,
            iowait: 0,
            irq: 0,
            softirq: 0,
        };
        let pct = cpu_pct(&a, &b);
        // Busy delta = 250, total delta = 1000, so 25%.
        assert!((pct - 25.0_f32).abs() < 1.0);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn havoc_fuzz_parse_proc_stat(s in ".*") {
            let _ = parse_proc_stat_sample(&s);
        }

        #[test]
        fn havoc_fuzz_parse_proc_meminfo(s in ".*") {
            let _ = parse_proc_meminfo(&s);
        }

        #[test]
        fn havoc_fuzz_parse_proc_diskstats(s in ".*") {
            let _ = parse_proc_diskstats(&s);
        }

        #[test]
        fn havoc_fuzz_parse_proc_net_dev(s in ".*") {
            let _ = parse_proc_net_dev(&s);
        }

    }

    #[test]
    fn havoc_fuzz_parse_proc_meminfo_no_panic() {
        let _ = parse_proc_meminfo(
            "MemTotal: 9999999999999999999\nMemFree: 9999999999999999999\nBuffers: 9999999999999999999\nCached: 9999999999999999999\nSwapTotal: 9999999999999999999\nSwapFree: 9999999999999999999\n",
        );
    }

    #[test]
    fn havoc_fuzz_parse_proc_diskstats_no_panic() {
        let _ = parse_proc_diskstats(
            " 8       0 sda 9999999999999999999 0 9999999999999999999 0 9999999999999999999 0 9999999999999999999\n",
        );
    }
}

#[test]
fn parse_meminfo_empty() {
    let mem = parse_proc_meminfo("");
    assert!(mem.is_none());
}

#[test]
fn parse_meminfo_missing_total() {
    let mem = parse_proc_meminfo("MemFree: 1024 kB\n");
    assert!(mem.is_none());
}

#[test]
fn cpu_sample_overflow_protection() {
    let a = CpuSample {
        user: u64::MAX,
        nice: 0,
        system: 0,
        idle: 0,
        iowait: 0,
        irq: 0,
        softirq: 0,
    };
    let b = CpuSample {
        user: 0, // Wrapped or invalid
        nice: 0,
        system: 0,
        idle: 0,
        iowait: 0,
        irq: 0,
        softirq: 0,
    };
    let pct = cpu_pct(&a, &b);
    assert!((pct - 0.0).abs() < f32::EPSILON);
}
