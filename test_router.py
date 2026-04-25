with open('crates/hitz-daemon/src/router.rs', 'r') as f:
    data = f.read()

data = data.replace("""    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,""",
"""    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,""")

with open('crates/hitz-daemon/src/router.rs', 'w') as f:
    f.write(data)
