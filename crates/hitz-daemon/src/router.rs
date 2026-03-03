//! HTTP request router — dispatches to `VmManager` methods.

// Response::builder() only fails on invalid header names/values;
// ours are hardcoded constants, so `.expect()` is safe here.
#![allow(clippy::expect_used)]

use std::convert::Infallible;

use bytes::Bytes;
use hitz_api::{ActionVmRequest, ApiError, CreateVmRequest, VmAction};
use hitz_hal::Hypervisor;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response, StatusCode, body::Incoming};

use crate::error::DaemonError;
use crate::vm_manager::VmManager;

/// Route an incoming HTTP request to the appropriate handler.
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let result = match (method.clone(), path.as_str()) {
        (Method::GET, "/vms") => handle_list(manager),
        _ if path.starts_with("/vms/") => {
            let segments: Vec<&str> = path.splitn(4, '/').collect();
            // segments: ["", "vms", "{id}", "action"?]
            match segments.get(2) {
                Some(id) if !id.is_empty() => {
                    let suffix = segments.get(3).copied();
                    route_vm(req, &method, id, suffix, manager).await
                }
                _ => Ok(error_response(StatusCode::BAD_REQUEST, "missing VM ID")),
            }
        }
        _ => Ok(error_response(StatusCode::NOT_FOUND, "not found")),
    };

    Ok(result.unwrap_or_else(|e| daemon_error_response(&e)))
}

async fn route_vm<H>(
    req: Request<Incoming>,
    method: &Method,
    id: &str,
    suffix: Option<&str>,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    match (method, suffix) {
        (&Method::PUT, None) => handle_create(req, id, manager).await,
        (&Method::GET, None) => handle_get(id, manager),
        (&Method::DELETE, None) => handle_delete(id, manager),
        (&Method::POST, Some("action")) => handle_action(req, id, manager).await,
        _ => Ok(error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        )),
    }
}

async fn handle_create<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let create_req: CreateVmRequest = serde_json::from_slice(&body.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = manager.create_vm(id.to_string(), create_req.config)?;
    json_response(StatusCode::CREATED, &info)
}

fn handle_get<H>(id: &str, manager: &VmManager<H>) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let info = manager.get_vm(id)?;
    json_response(StatusCode::OK, &info)
}

fn handle_list<H>(manager: &VmManager<H>) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let list = manager.list_vms()?;
    json_response(StatusCode::OK, &list)
}

fn handle_delete<H>(id: &str, manager: &VmManager<H>) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    manager.delete_vm(id)?;
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Full::new(Bytes::new()))
        .expect("build empty response"))
}

async fn handle_action<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<Full<Bytes>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let action_req: ActionVmRequest = serde_json::from_slice(&body.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = match action_req.action {
        VmAction::Start => manager.start_vm(id)?,
        VmAction::Stop => manager.stop_vm(id)?,
    };
    json_response(StatusCode::OK, &info)
}

fn json_response<T: serde::Serialize>(
    status: StatusCode,
    body: &T,
) -> Result<Response<Full<Bytes>>, DaemonError> {
    let json = serde_json::to_string(body)
        .map_err(|e| DaemonError::Internal(format!("JSON serialize: {e}")))?;
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .expect("build json response"))
}

fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    let body = ApiError {
        message: message.to_string(),
    };
    let json = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"message":"internal error"}"#.to_string());
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .expect("build error response")
}

fn daemon_error_response(err: &DaemonError) -> Response<Full<Bytes>> {
    let status = match err {
        DaemonError::NotFound(_) => StatusCode::NOT_FOUND,
        DaemonError::AlreadyExists(_) | DaemonError::InvalidState { .. } => StatusCode::CONFLICT,
        DaemonError::Vmm(hitz_vmm::VmError::Config(_)) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error_response(status, &err.to_string())
}
