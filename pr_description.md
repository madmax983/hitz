💡 What: Optimized the HTTP request routing in `crates/hitz-daemon/src/router.rs` by utilizing `req.into_parts()` to extract the HTTP method and URI path, avoiding a clone of the `Method` and the heap allocation involved in converting the path to a `String`. We also passed the remaining `body: Incoming` to sub-handlers, eliminating `into_body()` chains.

🎯 Why: Every incoming API request cloned the HTTP method and allocated a new String for the URI path on the hot routing path, which adds unnecessary heap allocations and overhead for a microVM daemon.

📊 Impact: Removes one `.clone()` on `hyper::Method` and one `.to_string()` heap allocation for the `Uri` per incoming request.

🔬 Measurement: Verify tests run clean, `cargo check` and `cargo test -p hitz-daemon`.
