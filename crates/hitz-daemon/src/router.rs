//! HTTP request router — dispatches to `VmManager` methods.

// Response::builder() only fails on invalid header names/values;
// ours are hardcoded constants, so `.expect()` is safe here.
#![allow(clippy::expect_used)]

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use hitz_api::{ActionVmRequest, ApiError, CloneVmRequest, CreateVmRequest, VmAction};
use hitz_hal::Hypervisor;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Frame};
use hyper::{Method, Request, Response, StatusCode, body::Incoming};
use tracing::Instrument as _;

use crate::error::DaemonError;
use crate::vm_manager::VmManager;

/// Streaming body backed by a tokio mpsc channel.
struct ChannelBody {
    rx: tokio::sync::mpsc::Receiver<Bytes>,
}

impl Body for ChannelBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrap a `Full<Bytes>` body into a boxed body.
fn full_boxed(body: Full<Bytes>) -> BoxBody<Bytes, Infallible> {
    body.map_err(|never| match never {}).boxed()
}

/// Route an incoming HTTP request to the appropriate handler.
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    // ⚡ Bolt Optimization: Avoid cloning the HTTP method and allocating a String for the URI path.
    // Instead, use `let (parts, body) = req.into_parts();` to cheaply borrow `parts.method`
    // and `parts.uri.path()` while passing ownership of the `body` to sub-handlers.
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let path = parts.uri.path();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,
        http.status_code = tracing::field::Empty,
    );

    let result = async {
        match (&method, path) {
            (&Method::GET, "/vms") => handle_list(manager),
            _ if path.starts_with("/vms/") => {
                let mut segments = path.splitn(4, '/');
                // segments: ["", "vms", "{id}", "action"?]
                match segments.nth(2) {
                    Some(id) if !id.is_empty() => {
                        let suffix = segments.next();
                        route_vm(body, &method, id, suffix, manager).await
                    }
                    _ => Ok(error_response(StatusCode::BAD_REQUEST, "missing VM ID")),
                }
            }
            _ => Ok(error_response(StatusCode::NOT_FOUND, "not found")),
        }
    }
    .instrument(span.clone())
    .await;

    let response = result.unwrap_or_else(|e| daemon_error_response(&e));

    let _ = span.record("http.status_code", response.status().as_u16());

    Ok(response)
}

async fn route_vm<H>(
    body: Incoming,
    method: &Method,
    id: &str,
    suffix: Option<&str>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    match (method, suffix) {
        (&Method::PUT, None) => handle_create(body, id, manager).await,
        (&Method::GET, None) => handle_get(id, manager),
        (&Method::GET, Some("serial")) => handle_serial(id, manager),
        (&Method::GET, Some("metrics")) => handle_metrics(id, manager),
        (&Method::DELETE, None) => handle_delete(id, manager),
        (&Method::POST, Some("action")) => handle_action(body, id, manager).await,
        (&Method::POST, Some("clone")) => handle_clone(body, id, manager).await,
        _ => Ok(error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        )),
    }
}

async fn handle_create<H>(
    body: Incoming,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body_collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let create_req: CreateVmRequest = serde_json::from_slice(&body_collected.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = manager.create_vm(id.to_string(), &create_req.config)?;
    json_response(StatusCode::CREATED, &info)
}

fn handle_get<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let info = manager.get_vm(id)?;
    json_response(StatusCode::OK, &info)
}

fn handle_list<H>(
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let list = manager.list_vms()?;
    json_response(StatusCode::OK, &list)
}

fn handle_delete<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    manager.delete_vm(id)?;
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(full_boxed(Full::new(Bytes::new())))
        .expect("build empty response"))
}

async fn handle_action<H>(
    body: Incoming,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body_collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let action_req: ActionVmRequest = serde_json::from_slice(&body_collected.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = match action_req.action {
        VmAction::Start => manager.start_vm(id)?,
        VmAction::Stop => manager.stop_vm(id)?,
        VmAction::Restart => manager.restart_vm(id).await?,
    };
    json_response(StatusCode::OK, &info)
}

async fn handle_clone<H>(
    body: Incoming,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body_collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let clone_req: CloneVmRequest = serde_json::from_slice(&body_collected.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = manager.clone_vm(id, &clone_req.dest_id)?;
    json_response(StatusCode::CREATED, &info)
}

fn handle_serial<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let mut reader = manager.serial_reader(id)?;
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(64);

    drop(tokio::spawn(async move {
        while let Some(chunk) = reader.read_chunk().await {
            if tx.send(Bytes::from(chunk)).await.is_err() {
                break;
            }
        }
    }));

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("transfer-encoding", "chunked")
        .body(ChannelBody { rx }.boxed())
        .expect("build serial response"))
}

fn handle_metrics<H>(
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    manager.request_metrics_snapshot(id).map_or_else(
        || {
            Ok(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "vsock metrics not available (push-to-OTel mode active; on-demand pull not yet implemented)",
            ))
        },
        |snap| json_response(StatusCode::OK, &snap),
    )
}

fn json_response<T: serde::Serialize>(
    status: StatusCode,
    body: &T,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError> {
    let json = serde_json::to_string(body)
        .map_err(|e| DaemonError::Internal(format!("JSON serialize: {e}")))?;
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(full_boxed(Full::new(Bytes::from(json))))
        .expect("build json response"))
}

fn error_response(status: StatusCode, message: &str) -> Response<BoxBody<Bytes, Infallible>> {
    let body = ApiError {
        message: message.to_string(),
    };
    let json = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"message":"internal error"}"#.to_string());
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(full_boxed(Full::new(Bytes::from(json))))
        .expect("build error response")
}

fn daemon_error_response(err: &DaemonError) -> Response<BoxBody<Bytes, Infallible>> {
    let status = match err {
        DaemonError::NotFound(_) => StatusCode::NOT_FOUND,
        DaemonError::AlreadyExists(_) | DaemonError::InvalidState { .. } => StatusCode::CONFLICT,
        DaemonError::Vmm(hitz_vmm::VmError::Config(_)) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error_response(status, &err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hitz_api::VmState;

    /// Compile guard: the `route()` function's span instrumentation must not
    /// break its infallible return type, and must compile with tracing imports.
    #[tokio::test]
    async fn route_span_compiles() {
        // This test just verifies the file compiles with span instrumentation.
        // We can't call route() without a real VmManager, so we test the
        // imports compile by referencing the span name as a constant.
        let _ = "daemon.request";
    }

    async fn extract_body_string(body: BoxBody<Bytes, Infallible>) -> String {
        use http_body_util::BodyExt;
        let collected = body.collect().await.expect("collect body");
        let bytes = collected.to_bytes();
        String::from_utf8(bytes.to_vec()).expect("valid utf8")
    }

    #[tokio::test]
    async fn should_format_error_response() {
        let resp = error_response(StatusCode::BAD_REQUEST, "invalid input data");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .expect("content type header missing")
                .to_str()
                .expect("content type is valid string"),
            "application/json"
        );

        let body_str = extract_body_string(resp.into_body()).await;
        assert_eq!(body_str, r#"{"message":"invalid input data"}"#);
    }

    #[tokio::test]
    async fn should_map_daemon_errors_to_status_codes() {
        struct TestCase {
            err: DaemonError,
            expected_status: StatusCode,
            expected_message_contains: &'static str,
        }

        let cases = vec![
            TestCase {
                err: DaemonError::NotFound("vm-1".to_string()),
                expected_status: StatusCode::NOT_FOUND,
                expected_message_contains: "VM not found: vm-1",
            },
            TestCase {
                err: DaemonError::AlreadyExists("vm-1".to_string()),
                expected_status: StatusCode::CONFLICT,
                expected_message_contains: "VM already exists: vm-1",
            },
            TestCase {
                err: DaemonError::InvalidState {
                    id: "vm-1".to_string(),
                    state: VmState::Running,
                    expected: "Stopped".to_string(),
                },
                expected_status: StatusCode::CONFLICT,
                expected_message_contains: "VM \\\"vm-1\\\" is Running, expected Stopped",
            },
            TestCase {
                err: DaemonError::Vmm(hitz_vmm::VmError::Config("bad config".to_string())),
                expected_status: StatusCode::BAD_REQUEST,
                expected_message_contains: "VMM error: Invalid configuration: bad config",
            },
            TestCase {
                err: DaemonError::Vmm(hitz_vmm::VmError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "disk missing",
                ))),
                expected_status: StatusCode::INTERNAL_SERVER_ERROR,
                expected_message_contains: "VMM error: I/O error: disk missing",
            },
            TestCase {
                err: DaemonError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file missing",
                )),
                expected_status: StatusCode::INTERNAL_SERVER_ERROR,
                expected_message_contains: "I/O error: file missing",
            },
            TestCase {
                err: DaemonError::Internal("something exploded".to_string()),
                expected_status: StatusCode::INTERNAL_SERVER_ERROR,
                expected_message_contains: "internal error: something exploded",
            },
        ];

        for case in cases {
            let resp = daemon_error_response(&case.err);
            assert_eq!(
                resp.status(),
                case.expected_status,
                "Failed on error: {:?}",
                case.err
            );

            let body_str = extract_body_string(resp.into_body()).await;
            assert!(
                body_str.contains(case.expected_message_contains),
                "Expected body to contain '{}', but got: {}",
                case.expected_message_contains,
                body_str
            );
        }
    }
}
