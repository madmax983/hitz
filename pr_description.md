💡 What: Replaced intermediate vector allocations and string joins with iterator chains and std::fmt::Write in hitz-cli's health monitor.
🎯 Why: The monitor loop unnecessarily allocated temporary Vecs and performed multiple format! heap allocations per frame/update.
📊 Impact: Eliminates several heap allocations per monitor loop iteration.
🔭 Measurement: Review monitor_health function for zero-allocation formatting.
