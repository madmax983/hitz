//! Named pipe HTTP server.
//!
//! Listens on a Windows named pipe and serves HTTP/1.1 requests via hyper.
//! Each client connection gets its own tokio task.

use hitz_hal::Hypervisor;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::windows::named_pipe::ServerOptions;

use crate::router;
use crate::vm_manager::VmManager;

/// Run the named pipe HTTP server.
///
/// Blocks until the future is canceled (typically by `tokio::select!` with
/// Ctrl+C). Each client connection is handled in a spawned task.
pub async fn run_server<H>(pipe_path: &str, manager: VmManager<H>) -> Result<(), std::io::Error>
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
                tracing::error!("connection error: {e}");
            }
        }));
    }
}
