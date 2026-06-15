#!/bin/bash
sed -i 's/exit_reason: Option<String>,/exit_reason: Option<ExitReason>,/' crates/hitz-daemon/src/vm_manager.rs
