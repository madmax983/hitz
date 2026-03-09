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
    CpuSample, cpu_pct, parse_proc_diskstats, parse_proc_meminfo, parse_proc_net_dev,
    parse_proc_stat_sample,
};

const PUSH_INTERVAL_SECS: u64 = 5;
const TOP_N_PROCS: usize = 10;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn collect_snapshot() -> MetricsSnapshot {
    let sample1 = parse_proc_stat_sample(&read_file("/proc/stat"));
    thread::sleep(Duration::from_millis(100));
    let sample2 = parse_proc_stat_sample(&read_file("/proc/stat"));

    let total_pct = sample1
        .first()
        .zip(sample2.first())
        .map(|(a, b)| cpu_pct(a, b))
        .unwrap_or(0.0);

    let per_core: Vec<f32> = sample1
        .iter()
        .skip(1)
        .zip(sample2.iter().skip(1))
        .map(|(a, b)| cpu_pct(a, b))
        .collect();

    let load_avg = parse_load_avg(&read_file("/proc/loadavg"));
    let memory = parse_proc_meminfo(&read_file("/proc/meminfo")).unwrap_or_else(|| {
        hitz_api::MemoryMetrics {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            buffers_bytes: 0,
            cached_bytes: 0,
            swap_total: 0,
            swap_used: 0,
        }
    });
    let disks = parse_proc_diskstats(&read_file("/proc/diskstats"));
    let networks = parse_proc_net_dev(&read_file("/proc/net/dev"));
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

fn collect_top_procs(n: usize) -> Vec<hitz_api::ProcMetrics> {
    let mut procs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(pid_str) = path.file_name().and_then(|n| n.to_str()) {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    let stat_path = format!("/proc/{pid}/stat");
                    if let Ok(stat) = std::fs::read_to_string(&stat_path) {
                        if let Some(p) = parse_proc_pid_stat(pid, &stat) {
                            procs.push(p);
                        }
                    }
                }
            }
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

fn parse_proc_pid_stat(pid: u32, content: &str) -> Option<hitz_api::ProcMetrics> {
    let open = content.find('(')?;
    let close = content.rfind(')')?;
    let name = content[open + 1..close].to_string();
    let rest: Vec<&str> = content[close + 2..].split_ascii_whitespace().collect();
    let state = rest.first()?.chars().next().unwrap_or('?');
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let rss_pages: u64 = rest.get(21)?.parse().ok()?;
    #[allow(clippy::cast_precision_loss)]
    let cpu_pct = (utime + stime) as f32 / 100.0;
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
    let encoded =
        rmp_serde::to_vec(snap).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = (encoded.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)
}

#[cfg(unix)]
fn push_loop() {
    loop {
        thread::sleep(Duration::from_secs(PUSH_INTERVAL_SECS));
        let snap = collect_snapshot();
        let addr = vsock::VsockAddr::new(VMADDR_CID_HOST, VSOCK_METRICS_PORT);
        if let Ok(mut stream) = vsock::VsockStream::connect(&addr) {
            let _ = send_snapshot(&mut stream, &snap);
        }
    }
}

#[cfg(unix)]
fn pull_server() {
    use std::io::Read;
    let listener =
        match vsock::VsockListener::bind_with_cid_port(vsock::VMADDR_CID_ANY, VSOCK_METRICS_PORT) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("hitz-agent: failed to bind vsock: {e}");
                return;
            }
        };
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let mut req_byte = [0u8; 1];
            if stream.read_exact(&mut req_byte).is_ok() {
                let snap = collect_snapshot();
                let _ = send_snapshot(&mut stream, &snap);
            }
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
