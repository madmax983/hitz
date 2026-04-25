with open('crates/hitz-daemon/src/router.rs', 'r') as f:
    data = f.read()

data = data.replace("""    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::info_span!(
        "daemon.request",
        http.method = %method,
        http.route = %path,""",
"""    let span = tracing::info_span!(
        "daemon.request",
        http.method = %req.method(),
        http.route = %req.uri().path(),""")

data = data.replace("""    let result = async {
        match (&method, path.as_str()) {""",
"""    let result = async {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        match (&method, path.as_str()) {""")

with open('crates/hitz-daemon/src/router.rs', 'w') as f:
    f.write(data)
