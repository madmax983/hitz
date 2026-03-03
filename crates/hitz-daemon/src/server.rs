//! Named pipe and TCP HTTP server.
//!
//! Listens on a Windows named pipe (always) and optionally on a TCP socket.
//! Both transports serve HTTP/1.1 requests via hyper using the same router.
//! Each client connection gets its own tokio task.

use std::net::SocketAddr;

use hitz_hal::Hypervisor;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::net::windows::named_pipe::ServerOptions;

use crate::router;
use crate::vm_manager::VmManager;

/// Run the named pipe HTTP server, and optionally a TCP listener.
///
/// Blocks until the future is canceled (typically by `tokio::select!` with
/// Ctrl+C). Each client connection is handled in a spawned task.
///
/// When `tcp_addr` is `Some`, a TCP listener is spawned as a parallel task
/// sharing the same router and `VmManager`.
pub async fn run_server<H>(
    pipe_path: &str,
    tcp_addr: Option<SocketAddr>,
    manager: VmManager<H>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    if let Some(addr) = tcp_addr {
        let mgr = manager.clone();
        drop(tokio::spawn(async move {
            if let Err(e) = run_tcp_listener(addr, mgr).await {
                tracing::error!("TCP listener error: {e}");
            }
        }));
    }

    run_pipe_listener(pipe_path, manager).await
}

/// Accept connections on a Windows named pipe and serve HTTP/1.1.
async fn run_pipe_listener<H>(pipe_path: &str, manager: VmManager<H>) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let mut first = true;

    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(first)
            .create(pipe_path)?;
        first = false;

        // Wait for a client to connect.
        server.connect().await?;
        tracing::debug!("client connected to {pipe_path}");

        let mgr = manager.clone();
        drop(tokio::spawn(async move {
            let io = TokioIo::new(server);
            let service = service_fn(move |req| {
                let m = mgr.clone();
                async move { router::route(req, &m).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::error!("pipe connection error: {e}");
            }
        }));
    }
}

/// Accept connections on a TCP socket and serve HTTP/1.1.
async fn run_tcp_listener<H>(addr: SocketAddr, manager: VmManager<H>) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("TCP listener bound to {addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("TCP client connected from {peer}");

        let mgr = manager.clone();
        drop(tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let m = mgr.clone();
                async move { router::route(req, &m).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::error!("TCP connection error ({peer}): {e}");
            }
        }));
    }
}
