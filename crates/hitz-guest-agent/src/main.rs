//! Hitz guest metrics agent.
//!
//! Runs inside the guest VM. Connects to the host via virtio-vsock
//! and streams resource snapshots every 5 seconds.

#![allow(clippy::unwrap_used)]
// guest agent uses unwrap for simplicity in non-critical paths
// On non-unix platforms (e.g. Windows host builds for testing), most of the
// runtime code is dead. Suppress the noise; the /proc parsers are tested via
// the `proc` module's own unit tests.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hitz_api::{CpuMetrics, MetricsSnapshot, VMADDR_CID_HOST, VSOCK_METRICS_PORT};

mod proc;
use proc::{
    cpu_pct, parse_proc_diskstats, parse_proc_meminfo, parse_proc_net_dev, parse_proc_stat_sample,
};

const PUSH_INTERVAL_SECS: u64 = 5;
const TOP_N_PROCS: usize = 10;

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

// ⚡ Bolt: We pass a mutable String buffer to reuse the same heap allocation
// for every `/proc` file we read during a metrics snapshot. This eliminates
// 6 string allocations per frame.
fn read_file_into(path: &str, buf: &mut String) {
    use std::io::Read;
    buf.clear();
    let _ = std::fs::File::open(path).and_then(|f| f.take(1024 * 1024).read_to_string(buf));
}

fn collect_snapshot(buf: &mut String) -> MetricsSnapshot {
    read_file_into("/proc/stat", buf);
    let sample1 = parse_proc_stat_sample(buf);
    thread::sleep(Duration::from_millis(100));
    read_file_into("/proc/stat", buf);
    let sample2 = parse_proc_stat_sample(buf);

    let total_pct = sample1
        .first()
        .zip(sample2.first())
        .map_or(0.0, |(a, b)| cpu_pct(a, b));

    let per_core: Vec<f32> = sample1
        .iter()
        .skip(1)
        .zip(sample2.iter().skip(1))
        .map(|(a, b)| cpu_pct(a, b))
        .collect();

    read_file_into("/proc/loadavg", buf);
    let load_avg = parse_load_avg(buf);
    read_file_into("/proc/meminfo", buf);
    let memory = parse_proc_meminfo(buf).unwrap_or(hitz_api::MemoryMetrics {
        total_bytes: 0,
        used_bytes: 0,
        free_bytes: 0,
        buffers_bytes: 0,
        cached_bytes: 0,
        swap_total: 0,
        swap_used: 0,
    });
    read_file_into("/proc/diskstats", buf);
    let disks = parse_proc_diskstats(buf);
    read_file_into("/proc/net/dev", buf);
    let networks = parse_proc_net_dev(buf);
    let processes = collect_top_procs(TOP_N_PROCS);

    MetricsSnapshot {
        timestamp_ms: now_ms(),
        cpu: CpuMetrics {
            total_pct,
            per_core,
            load_avg,
        },
        memory,
        disks,
        networks,
        processes,
    }
}

fn parse_load_avg(content: &str) -> [f32; 3] {
    let mut parts = content.split_ascii_whitespace();
    let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    [a, b, c]
}

/// Collects top processes by CPU usage.
///
/// Pre-allocating the vector and reusing `String` buffers prevents allocating
/// new memory strings `2 * N` times per collection frame, greatly reducing heap
/// fragmentation on the guest agent.
fn collect_top_procs(n: usize) -> Vec<hitz_api::ProcMetrics> {
    let mut procs = Vec::with_capacity(256);
    let mut path_buf = String::with_capacity(32);
    let mut stat_buf = String::with_capacity(1024);

    if let Ok(entries) = std::fs::read_dir("/proc") {
        use std::fmt::Write;
        use std::io::Read;
        for entry in entries.filter_map(std::result::Result::ok) {
            let file_name = entry.file_name();

            let Some(pid_str) = file_name.to_str() else {
                continue;
            };
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };

            path_buf.clear();
            let _ = write!(path_buf, "/proc/{pid}/stat");

            stat_buf.clear();

            let Ok(f) = std::fs::File::open(&path_buf) else {
                continue;
            };
            if f.take(1024 * 1024).read_to_string(&mut stat_buf).is_err() {
                continue;
            }
            let Some(p) = parse_proc_pid_stat(pid, &stat_buf) else {
                continue;
            };

            procs.push(p);
        }
    }
    procs.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    procs.truncate(n);
    procs
}

/// Read wall-clock uptime in seconds from `/proc/uptime`.
///
/// Returns `1.0` as a safe fallback if the file is unreadable (e.g. on Windows).
fn read_uptime_secs() -> f64 {
    use std::io::Read;
    let mut s = String::new();
    std::fs::File::open("/proc/uptime")
        .and_then(|f| f.take(1024).read_to_string(&mut s))
        .ok()
        .and_then(|_| s.split_ascii_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(1.0)
}

fn parse_proc_pid_stat(pid: u32, content: &str) -> Option<hitz_api::ProcMetrics> {
    let open = content.find('(')?;
    let close = content.rfind(')')?;
    let name = content.get(open + 1..close)?.to_string();
    let remaining = content.get(close + 2..)?;
    let mut iter = remaining.split_ascii_whitespace();

    // According to proc(5) for /proc/[pid]/stat:
    // (1) pid
    // (2) comm
    // (3) state (rest[0] after our split)
    let state = iter.next()?.chars().next().unwrap_or('?');

    // (4) ppid to (13) minflt -> 10 fields to skip to get to utime
    let utime: u64 = iter.nth(10)?.parse().ok()?; // (14) utime
    let stime: u64 = iter.next()?.parse().ok()?; // (15) stime

    // (16) cutime to (23) vsize -> 8 fields to skip to get to rss
    let rss_pages: u64 = iter.nth(8)?.parse().ok()?; // (24) rss
    // cpu_pct: lifetime CPU fraction as a percentage.
    // Computed as (lifetime_ticks / USER_HZ) / uptime_secs * 100, capped at 100%.
    // USER_HZ = 100 on all Linux targets.
    // This is a lifetime average, not a current-window percentage.
    // Future: implement two-sample differential for accurate current usage.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let uptime_ticks = (read_uptime_secs() * 100.0) as u64;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let cpu_pct: f32 = if uptime_ticks == 0 {
        0.0_f32
    } else {
        (((utime + stime) as f64 / uptime_ticks as f64) * 100.0).clamp(0.0, 100.0) as f32
    };
    Some(hitz_api::ProcMetrics {
        pid,
        name,
        cpu_pct,
        rss_bytes: rss_pages * 4096,
        state,
    })
}

#[cfg(unix)]
fn send_snapshot(stream: &mut vsock::VsockStream, snap: &MetricsSnapshot) -> std::io::Result<()> {
    use std::io::Write;
    let encoded = rmp_serde::to_vec(snap).map_err(std::io::Error::other)?;
    let len = u32::try_from(encoded.len())
        .unwrap_or(u32::MAX)
        .to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)
}

#[cfg(unix)]
fn push_loop() {
    let mut buf = String::with_capacity(4096);
    loop {
        thread::sleep(Duration::from_secs(PUSH_INTERVAL_SECS));
        let snap = collect_snapshot(&mut buf);
        let addr = vsock::VsockAddr::new(VMADDR_CID_HOST, VSOCK_METRICS_PORT);
        if let Ok(mut stream) = vsock::VsockStream::connect(&addr) {
            let _ = send_snapshot(&mut stream, &snap);
        }
    }
}

#[cfg(unix)]
fn pull_server() {
    use std::io::Read;
    let mut buf = String::with_capacity(4096);
    let listener =
        match vsock::VsockListener::bind_with_cid_port(vsock::VMADDR_CID_ANY, VSOCK_METRICS_PORT) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("hitz-agent: failed to bind vsock: {e}");
                return;
            }
        };
    for mut stream in listener.incoming().flatten() {
        let mut req_byte = [0u8; 1];
        if stream.read_exact(&mut req_byte).is_ok() {
            let snap = collect_snapshot(&mut buf);
            let _ = send_snapshot(&mut stream, &snap);
        }
    }
}

fn main() {
    #[cfg(unix)]
    {
        let push_handle = thread::spawn(push_loop);
        let pull_handle = thread::spawn(pull_server);
        let _ = push_handle.join();
        let _ = pull_handle.join();
    }
    #[cfg(not(unix))]
    {
        eprintln!("hitz-agent: not supported on this platform");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn havoc_fuzz_parse_proc_pid_stat(s in ".*\\(.*\\).*") {
            let _ = parse_proc_pid_stat(1, &s);
        }
    }

    #[test]
    fn havoc_test_parse_proc_pid_stat_out_of_bounds() {
        let content = "(a)";
        assert!(parse_proc_pid_stat(1, content).is_none());
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_parse_load_avg() {
        assert_eq!(parse_load_avg("1.23 4.56 7.89"), [1.23, 4.56, 7.89]);
        assert_eq!(parse_load_avg("1.23 4.56"), [1.23, 4.56, 0.0]);
        assert_eq!(parse_load_avg("1.23"), [1.23, 0.0, 0.0]);
        assert_eq!(parse_load_avg(""), [0.0, 0.0, 0.0]);
        assert_eq!(parse_load_avg("invalid"), [0.0, 0.0, 0.0]);
        assert_eq!(parse_load_avg("1.23 invalid 7.89"), [1.23, 0.0, 7.89]);
    }

    #[test]
    fn test_parse_proc_pid_stat() {
        // A well-formed /proc/[pid]/stat line.
        // We only care about:
        // (1) pid
        // (2) comm
        // (3) state -> index 2 after split
        // (14) utime -> index 13
        // (15) stime -> index 14
        // (24) rss -> index 23
        // Note: iter starts AFTER comm, so:
        // state = iter[0]
        // utime = iter[11]  (10 fields skipped)
        // stime = iter[12]
        // rss = iter[21] (8 fields skipped)

        let content = "123 (my_process) S 1 1 1 1 1 1 1 1 1 1 100 200 1 1 1 1 1 1 1 1 50";
        // utime = 100, stime = 200
        // rss = 50 pages -> 50 * 4096 = 204800 bytes

        // Since cpu_pct calculation involves uptime, we can't easily assert the exact
        // value without mocking uptime. But we can assert the other fields.
        let metrics = parse_proc_pid_stat(123, content).unwrap();

        assert_eq!(metrics.pid, 123);
        assert_eq!(metrics.name, "my_process");
        assert_eq!(metrics.state, 'S');
        assert_eq!(metrics.rss_bytes, 50 * 4096);

        // Test parsing with an empty name
        let content_empty_name = "123 () S 1 1 1 1 1 1 1 1 1 1 100 200 1 1 1 1 1 1 1 1 50";
        let metrics_empty_name = parse_proc_pid_stat(123, content_empty_name).unwrap();
        assert_eq!(metrics_empty_name.name, "");

        // Test parsing failure due to missing fields
        let content_short = "123 (short) S 1";
        assert!(parse_proc_pid_stat(123, content_short).is_none());
    }
}
