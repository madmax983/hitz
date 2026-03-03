//! Named pipe HTTP client for talking to the hitz daemon.

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::windows::named_pipe::ClientOptions;

/// Send an HTTP request to the daemon over the named pipe.
///
/// Returns `(status_code, response_body_string)`.
pub async fn pipe_request(
    pipe_path: &str,
    method: Method,
    path: &str,
    body: Option<&str>,
) -> Result<(StatusCode, String)> {
    let pipe = ClientOptions::new()
        .open(pipe_path)
        .with_context(|| format!("cannot connect to daemon at {pipe_path} — is it running?"))?;

    let io = TokioIo::new(pipe);
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
    let resp_body = resp
        .into_body()
        .collect()
        .await
        .context("read response body")?
        .to_bytes();
    let text = String::from_utf8_lossy(&resp_body).to_string();

    Ok((status, text))
}
