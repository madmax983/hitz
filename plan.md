1. Modify `crates/hitz-daemon/src/router.rs` using `replace_with_git_merge_diff` to implement the required zero-cost abstraction for the HTTP router.

We will apply the following diffs:

Diff 1 (update `route` and `route_vm` functions):
```
<<<<<<< SEARCH
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    // ⚡ Bolt Optimization: Avoid cloning the HTTP method.
    // It's cheaper to borrow it or let it remain bound to the request.
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,
        http.status_code = tracing::field::Empty,
    );

    let result = async {
        match (&method, path.as_str()) {
            (&Method::GET, "/vms") => handle_list(manager),
            _ if path.starts_with("/vms/") => {
                let mut segments = path.splitn(4, '/');
                // segments: ["", "vms", "{id}", "action"?]
                match segments.nth(2) {
                    Some(id) if !id.is_empty() => {
                        let suffix = segments.next();
                        route_vm(req, &method, id, suffix, manager).await
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
    req: Request<Incoming>,
    method: &Method,
    id: &str,
    suffix: Option<&str>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    match (method, suffix) {
        (&Method::PUT, None) => handle_create(req, id, manager).await,
        (&Method::GET, None) => handle_get(id, manager),
        (&Method::GET, Some("serial")) => handle_serial(id, manager),
        (&Method::GET, Some("metrics")) => handle_metrics(id, manager),
        (&Method::DELETE, None) => handle_delete(id, manager),
        (&Method::POST, Some("action")) => handle_action(req, id, manager).await,
        (&Method::POST, Some("clone")) => handle_clone(req, id, manager).await,
        _ => Ok(error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        )),
    }
}
=======
pub async fn route<H>(
    req: Request<Incoming>,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
where
    H: Hypervisor + Send + Sync + 'static,
{
    // ⚡ Bolt Optimization: Avoid cloning the HTTP method and path strings.
    // We deconstruct the request to borrow parts cheaply and pass the body to sub-handlers.
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
>>>>>>> REPLACE
```

Diff 2 (update `handle_create`, `handle_action`, and `handle_clone`):
```
<<<<<<< SEARCH
async fn handle_create<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
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
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
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
        VmAction::Restart => manager.restart_vm(id).await?,
    };
    json_response(StatusCode::OK, &info)
}

async fn handle_clone<H>(
    req: Request<Incoming>,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let clone_req: CloneVmRequest = serde_json::from_slice(&body.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = manager.clone_vm(id, &clone_req.dest_id)?;
    json_response(StatusCode::CREATED, &info)
}
=======
async fn handle_create<H>(
    body: Incoming,
    id: &str,
    manager: &VmManager<H>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, DaemonError>
where
    H: Hypervisor + Send + Sync + 'static,
{
    let collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let create_req: CreateVmRequest = serde_json::from_slice(&collected.to_bytes())
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
    let collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let action_req: ActionVmRequest = serde_json::from_slice(&collected.to_bytes())
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
    let collected = body
        .collect()
        .await
        .map_err(|e| DaemonError::Internal(format!("failed to read request body: {e}")))?;
    let clone_req: CloneVmRequest = serde_json::from_slice(&collected.to_bytes())
        .map_err(|e| DaemonError::Internal(format!("invalid JSON: {e}")))?;

    let info = manager.clone_vm(id, &clone_req.dest_id)?;
    json_response(StatusCode::CREATED, &info)
}
>>>>>>> REPLACE
```

2. Verify changes: We will use `run_in_bash_session` to run `cargo fmt`, `cargo clippy --all-targets --all-features --workspace --exclude hitz-whp --exclude hitz-daemon --exclude hitz-vmm --exclude hitz-cli --exclude hitz-net -- -D warnings`, and `cargo test --workspace --exclude hitz-whp --exclude hitz-daemon --exclude hitz-vmm --exclude hitz-cli --exclude hitz-net`.
Since `hitz-daemon` requires Windows-specific APIs, we will test it automatically via `wine` wrapper:
`CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo clippy --target x86_64-pc-windows-gnu -p hitz-daemon --all-targets --all-features -- -D warnings` and `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test --target x86_64-pc-windows-gnu -p hitz-daemon`.

3. Complete pre-commit steps to ensure proper testing, verification, review, and reflection are done.

4. Submit PR using `submit` tool with the exact title and description below:
    - title: "⚡ Bolt: [performance improvement]"
    - description: "💡 What: Removed cloning and allocating hyper request parts `Method` and `Uri` on the hot path by destructing the request with `into_parts()`.\n🎯 Why: The hitz-daemon HTTP router was allocating a `String` for the request path and calling `clone()` on the HTTP method for every incoming request. This was adding unnecessary allocations on the API request path.\n📊 Impact: Reduces string heap allocation per HTTP API request.\n🔬 Measurement: Run bench or `cargo test -p hitz-daemon` to ensure the routing remains functional."
