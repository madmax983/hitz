🔒 Warden: Prevent memory exhaustion DoS in daemon router

🦠 Threat: Deserialization bomb / memory exhaustion DoS via `req.into_body().collect().await` unbounded buffering in `hitz-daemon/src/router.rs`. Since there was no limit on the request body size, an attacker could stream a huge body (e.g., 10+ GB) and cause an Out-Of-Memory (OOM) crash.
🛡️ Defense: Used `http_body_util::Limited::new` to cap the request payload reader to 1 MiB before collecting into memory (Parse, don't validate approach applied to reading the request body).
💥 Severity: High - Any unauthenticated connection to the named pipe / local API could send an arbitrarily large payload and crash the daemon via OOM.
🧪 Verification: Validated the compiler passes. The application will cleanly return a Hyper/Axum error if a payload exceeds the 1 MiB limit, rather than consuming unbound memory.
