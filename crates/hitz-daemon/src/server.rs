//! Named pipe and TCP HTTP server.
//!
//! Listens on a Windows named pipe (always) and optionally on a TCP socket.
//! Both transports serve HTTP/1.1 requests via hyper using the same router.
//! Each client connection gets its own tokio task.
//!
//! Both listener loops observe a `tokio::sync::watch` shutdown signal and stop
//! accepting new connections when the sender broadcasts `true`.

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
/// Both listener loops exit cleanly when `shutdown` receives `true`. The
/// caller is responsible for sending the signal (e.g. on Ctrl+C) and then
/// awaiting this future.
///
/// When `tcp_addr` is `Some`, a TCP listener is spawned as a parallel task
/// sharing the same router and `VmManager`.
///
/// ## Examples
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use std::path::PathBuf;
/// # use hitz_daemon::{VmManager, run_server};
/// # use hitz_whp::WhpHypervisor;
/// #
/// # let hypervisor = Arc::new(WhpHypervisor::new().unwrap());
/// # let state_dir = PathBuf::from("C:\\hitz\\vms");
/// # let manager = VmManager::new(hypervisor, state_dir).unwrap();
/// # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
/// # rt.block_on(async {
/// let (tx, rx) = tokio::sync::watch::channel(false);
/// let server = run_server(
///     "\\\\.\\pipe\\my-pipe",
///     None,
///     manager,
///     rx,
/// );
/// // tokio::spawn(server);
/// // tx.send(true).unwrap(); // To shutdown
/// # });
/// ```
pub async fn run_server<H>(
    pipe_path: &str,
    tcp_addr: Option<SocketAddr>,
    manager: VmManager<H>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    if let Some(addr) = tcp_addr {
        let mgr = manager.clone();
        let mut sd = shutdown.clone();
        drop(tokio::spawn(async move {
            if let Err(e) = run_tcp_listener(addr, mgr, &mut sd).await {
                tracing::error!("TCP listener error: {e}");
            }
        }));
    }

    run_pipe_listener(pipe_path, manager, &mut shutdown).await
}

/// Accept connections on a Windows named pipe and serve HTTP/1.1.
///
/// Exits when `shutdown` receives `true`.
async fn run_pipe_listener<H>(
    pipe_path: &str,
    manager: VmManager<H>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let mut first = true;

    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(first)
            .create(pipe_path)?;
        first = false;

        tokio::select! {
            result = server.connect() => {
                result?;
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
            _ = shutdown.changed() => {
                tracing::info!("pipe listener shutting down");
                break;
            }
        }
    }
    Ok(())
}

/// Accept connections on a TCP socket and serve HTTP/1.1.
///
/// Exits when `shutdown` receives `true`.
async fn run_tcp_listener<H>(
    addr: SocketAddr,
    manager: VmManager<H>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("TCP listener bound to {addr}");

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, peer) = result?;
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
            _ = shutdown.changed() => {
                tracing::info!("TCP listener shutting down");
                break;
            }
        }
    }
    Ok(())
}
