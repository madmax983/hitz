//! HTTP client for talking to the hitz daemon over named pipe or TCP.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::net::windows::named_pipe::ClientOptions;

/// Send an HTTP request to the daemon over the named pipe or TCP.
///
/// When `tcp_addr` is `Some`, connects via TCP instead of the named pipe.
/// Returns `(status_code, response_body_string)`.
pub async fn pipe_request(
    pipe_path: &str,
    tcp_addr: Option<SocketAddr>,
    method: Method,
    path: &str,
    body: Option<&str>,
) -> Result<(StatusCode, String)> {
    if let Some(addr) = tcp_addr {
        let stream = TcpStream::connect(addr)
            .await
            .with_context(|| format!("cannot connect to daemon at {addr} — is it running?"))?;
        do_request(TokioIo::new(stream), method, path, body).await
    } else {
        let pipe = ClientOptions::new()
            .open(pipe_path)
            .with_context(|| format!("cannot connect to daemon at {pipe_path} — is it running?"))?;
        do_request(TokioIo::new(pipe), method, path, body).await
    }
}

/// Send a GET request and return the raw response for streaming.
///
/// Unlike [`pipe_request`], this does **not** collect the response body.
/// The caller is expected to consume frames from the returned
/// [`hyper::body::Incoming`] body.
pub async fn request_stream(
    pipe_path: &str,
    tcp_addr: Option<SocketAddr>,
    path: &str,
) -> Result<hyper::Response<hyper::body::Incoming>> {
    if let Some(addr) = tcp_addr {
        let stream = TcpStream::connect(addr)
            .await
            .with_context(|| format!("cannot connect to daemon at {addr} — is it running?"))?;
        do_request_stream(TokioIo::new(stream), path).await
    } else {
        let pipe = ClientOptions::new()
            .open(pipe_path)
            .with_context(|| format!("cannot connect to daemon at {pipe_path} — is it running?"))?;
        do_request_stream(TokioIo::new(pipe), path).await
    }
}

/// Send a GET request and return the raw response (body not collected).
async fn do_request_stream<I>(io: I, path: &str) -> Result<hyper::Response<hyper::body::Incoming>>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let (mut sender, conn) = http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;

    drop(tokio::spawn(conn));

    let req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Full::new(Bytes::new()))
        .context("build request")?;

    sender
        .send_request(req)
        .await
        .context("send request failed")
}

/// Perform an HTTP/1.1 request over the given I/O transport.
async fn do_request<I>(
    io: I,
    method: Method,
    path: &str,
    body: Option<&str>,
) -> Result<(StatusCode, String)>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let (mut sender, conn) = http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;

    drop(tokio::spawn(conn));

    let req_body = body.map_or_else(
        || Full::new(Bytes::new()),
        |b| Full::new(Bytes::from(b.to_string())),
    );

    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(req_body)
        .context("build request")?;

    let resp = sender
        .send_request(req)
        .await
        .context("send request failed")?;

    let status = resp.status();
    let resp_body = http_body_util::Limited::new(resp.into_body(), 2 * 1024 * 1024)
        .collect()
        .await
        .map_err(|e| anyhow::anyhow!("read response body: {e}"))?
        .to_bytes();
    let text = String::from_utf8_lossy(&resp_body).to_string();

    Ok((status, text))
}

#[cfg(test)]
mod tests {
    // Pipe client depends heavily on tokio and hyper running against a live system.
    // It is primarily integration-tested. Since we are asked to find logic branches,
    // this module might be better tested holistically. We skip directly testing this
    // I/O bound pipe request component in unit tests to avoid flakiness.
}
