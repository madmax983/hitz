//! TCP port forward manager — one listener task per rule.

use std::net::Ipv4Addr;
use std::sync::Arc;

use hitz_api::PortForward;
use opentelemetry::KeyValue;
use opentelemetry::global;
use tokio::task::JoinHandle;

/// Manages TCP port forward listeners for a single VM.
///
/// Each rule spawns a tokio task that accepts connections on `0.0.0.0:host_port`
/// and relays them to `guest_ip:guest_port` via [`tokio::io::copy_bidirectional`].
///
/// Dropping this manager aborts all listener tasks and makes a best-effort
/// attempt to abort any in-flight relay connections.
pub struct PortForwardManager {
    handles: Vec<JoinHandle<()>>,
    relay_handles: Arc<tokio::sync::Mutex<Vec<JoinHandle<()>>>>,
}

impl PortForwardManager {
    /// Start listener tasks for each rule.
    ///
    /// Bind failures are logged as warnings and skipped — they do not
    /// prevent the VM from starting.
    pub async fn start(guest_ip: Ipv4Addr, rules: &[PortForward]) -> Self {
        let mut handles = Vec::with_capacity(rules.len());
        let relay_handles: Arc<tokio::sync::Mutex<Vec<JoinHandle<()>>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let meter = global::meter("hitz");
        let connections_total = meter
            .u64_counter("hitz.portfwd.connections_total")
            .with_description("Total TCP connections accepted by port forwarders")
            .build();
        let relays_active = meter
            .i64_up_down_counter("hitz.portfwd.relays_active")
            .with_description("Currently active port-forward relay connections")
            .build();

        for rule in rules {
            let host_addr = std::net::SocketAddr::from(([0, 0, 0, 0], rule.host_port));
            let guest_addr = std::net::SocketAddr::from((guest_ip, rule.guest_port));

            let listener = match tokio::net::TcpListener::bind(host_addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!(
                        host_port = rule.host_port,
                        guest_port = rule.guest_port,
                        "port forward bind failed: {e}"
                    );
                    continue;
                }
            };

            tracing::info!(
                "port forward active: 0.0.0.0:{} → {guest_ip}:{}",
                rule.host_port,
                rule.guest_port
            );

            let relay_handles_clone = Arc::clone(&relay_handles);
            let connections_total_l = connections_total.clone();
            let relays_active_l = relays_active.clone();

            // ⚡ Bolt Optimization:
            // By creating a single `KeyValue` with an `i64` value instead of using `.to_string()`,
            // we eliminate 3 `String` heap allocations (`.clone()`) per incoming TCP connection on the hot path.
            let kv = KeyValue::new("host_port", i64::from(rule.host_port));

            let handle = tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((mut inbound, _peer)) => {
                            connections_total_l.add(1, std::slice::from_ref(&kv));
                            relays_active_l.add(1, std::slice::from_ref(&kv));

                            let relays_active_r = relays_active_l.clone();
                            let kv_r = kv.clone();
                            let relay = tokio::spawn(async move {
                                match tokio::net::TcpStream::connect(guest_addr).await {
                                    Ok(mut outbound) => {
                                        let _ = tokio::io::copy_bidirectional(
                                            &mut inbound,
                                            &mut outbound,
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "port forward connect to guest failed: {e}"
                                        );
                                    }
                                }
                                // Relay complete — decrement active counter.
                                relays_active_r.add(-1, &[kv_r]);
                            });
                            relay_handles_clone.lock().await.push(relay);
                        }
                        Err(e) => {
                            tracing::warn!("port forward accept error, retrying: {e}");
                        }
                    }
                }
            });

            handles.push(handle);
        }

        Self {
            handles,
            relay_handles,
        }
    }
}

impl Drop for PortForwardManager {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
        // Best-effort: abort relay tasks if the lock is immediately available.
        if let Ok(mut relays) = self.relay_handles.try_lock() {
            for handle in relays.drain(..) {
                handle.abort();
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::unused_async,
    unused_must_use
)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Helper: find a free port by binding to :0 then releasing.
    async fn free_port() -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn forwards_tcp_data() {
        // Start an echo server on a free port.
        let echo_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo_listener.local_addr().unwrap().port();

        // Echo server: read N bytes, write them back, close.
        let _echo_handle = tokio::spawn(async move {
            if let Ok((mut stream, _)) = echo_listener.accept().await {
                let mut buf = [0u8; 64];
                if let Ok(n) = stream.read(&mut buf).await {
                    let _ = stream.write_all(&buf[..n]).await;
                }
            }
        });

        // Forward a free host port → echo server (127.0.0.1:echo_port).
        let host_port = free_port().await;
        let rule = PortForward {
            host_port,
            guest_port: echo_port,
        };
        let _mgr = PortForwardManager::start(Ipv4Addr::LOCALHOST, &[rule]).await;

        // Give the listener task a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Connect via the forwarded port and send data.
        let mut conn = tokio::net::TcpStream::connect(format!("127.0.0.1:{host_port}"))
            .await
            .expect("connect to forwarded port");

        conn.write_all(b"hello").await.expect("write");
        let mut resp = [0u8; 5];
        let n = conn.read_exact(&mut resp).await.expect("read");
        assert_eq!(n, 5);
        assert_eq!(&resp, b"hello");
    }

    #[tokio::test]
    async fn skips_unavailable_host_port() {
        // Bind on the same wildcard address the manager uses so the port is
        // truly blocked.  On Windows, binding 127.0.0.1 does not conflict with
        // 0.0.0.0, so we must use 0.0.0.0 here.
        let blocker = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
        let blocked_port = blocker.local_addr().unwrap().port();

        // PortForwardManager should start without panicking, skipping the bad rule.
        let rule = PortForward {
            host_port: blocked_port,
            guest_port: 22,
        };
        let mgr = PortForwardManager::start(Ipv4Addr::LOCALHOST, &[rule]).await;
        assert!(mgr.handles.is_empty(), "should have skipped the bad bind");
    }

    #[tokio::test]
    async fn metrics_do_not_panic_without_provider() {
        // No OTel provider registered — all metric calls must be no-ops.
        // PortForwardManager::start must not panic when creating meter objects.
        let rule = PortForward {
            host_port: free_port().await,
            guest_port: 9998,
        };
        let mgr = PortForwardManager::start(Ipv4Addr::LOCALHOST, &[rule]).await;
        // Give listener a moment to start, then drop.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        drop(mgr);
    }

    #[tokio::test]
    async fn drop_aborts_listeners() {
        let host_port = free_port().await;
        let rule = PortForward {
            host_port,
            guest_port: 9999,
        };
        let mgr = PortForwardManager::start(Ipv4Addr::LOCALHOST, &[rule]).await;

        // Give the listener task a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Drop the manager — listener tasks are aborted.
        drop(mgr);

        // Give tokio a moment to clean up.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Now we should be able to rebind that port.
        let _listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{host_port}"))
            .await
            .expect("port should be free after manager dropped");
    }
}
