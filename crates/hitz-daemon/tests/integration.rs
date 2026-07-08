use hitz_api::{
    DEFAULT_GUEST_CID, DEFAULT_GUEST_IP, DEFAULT_HOST_IP, NetConfig, PortForward, VmConfig,
};
use hitz_daemon::VmManager;
use hitz_whp::WhpHypervisor;
use std::sync::Arc;

/// Phase 10: TCP port forward proxies connections to the guest.
///
/// Prerequisites:
/// - WHP enabled
/// - A Linux initramfs with a TCP listener on port 9999 (e.g. `nc -lp 9999`)
///   set up as PID 1 or started from init. Set env var
///   `HITZ_TEST_INITRAMFS` to the path.
/// - virtio-net working (Phase 7)
///
/// Boots a VM with `ports: [19999:9999]` and verifies it exits cleanly.
///
/// Skipped if `HITZ_TEST_INITRAMFS` is not set (env-var-gated).
#[tokio::test]
#[ignore = "requires WHP enabled (Hyper-V) and HITZ_TEST_INITRAMFS env var"]
async fn phase10_port_forward_tcp() {
    let initramfs_path = match std::env::var("HITZ_TEST_INITRAMFS") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            eprintln!("phase10_port_forward_tcp: skipped (set HITZ_TEST_INITRAMFS)");
            return;
        }
    };

    let kernel_path = std::path::PathBuf::from(
        std::env::var("HITZ_TEST_KERNEL").unwrap_or_else(|_| "vmlinux".into()),
    );

    let config = VmConfig {
        kernel_path,
        initramfs_path: Some(initramfs_path),
        disk_path: None,
        ram_mib: 256,
        cpus: 1,
        cmdline: Some("console=ttyS0 init=/init\0".into()),
        net: Some(NetConfig {
            mac: None,
            host_ip: DEFAULT_HOST_IP.into(),
            guest_ip: DEFAULT_GUEST_IP.into(),
            adapter_name: None,
        }),
        ports: vec![PortForward {
            host_port: 19999,
            guest_port: 9999,
        }],
        guest_cid: DEFAULT_GUEST_CID,
        guest_agent: hitz_api::GuestAgentMode::Auto,
    };

    let hypervisor = Arc::new(WhpHypervisor::new().expect("WHP not available"));
    let mut manager = VmManager::new(hypervisor);

    // Create the VM
    manager
        .create("test-vm", config)
        .expect("Failed to create VM");

    // Start the VM (this will start PortForwardManager too)
    manager.start("test-vm").await.expect("Failed to start VM");

    use std::io::{Read, Write};
    use std::net::TcpStream;

    // Retry loop for connecting to the HOST mapped port
    let mut stream = None;
    for _ in 0..20 {
        if let Ok(s) = TcpStream::connect("127.0.0.1:19999") {
            stream = Some(s);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let mut stream = stream.expect("Failed to connect to host-mapped port within timeout");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();

    let msg = b"Hello, Hitz!";
    stream.write_all(msg).expect("Failed to write to stream");

    let mut buf = [0u8; 12];
    stream
        .read_exact(&mut buf)
        .expect("Failed to read from stream");

    assert_eq!(&buf, msg, "Echoed data did not match");

    manager.stop("test-vm").await.expect("Failed to stop VM");
}
