#!/bin/bash
sed -i 's/pub fn with_capacity(capacity: usize) -> Self {/pub fn with_capacity(capacity: usize) -> Self {\n        assert!(capacity > 0, "capacity must be greater than 0");/' crates/hitz-vmm/src/serial_buf.rs
