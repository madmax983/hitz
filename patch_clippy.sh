#!/bin/bash
sed -i 's/    #\[must_use\]/    \/\/\/\n    \/\/\/ # Panics\n    \/\/\/\n    \/\/\/ Panics if `capacity` is 0.\n    #\[must_use\]/' crates/hitz-vmm/src/serial_buf.rs
sed -i 's/let read_pos = self.inner.lock().map(|i| i.total_written).unwrap_or(0);/let read_pos = self.inner.lock().map_or(0, |i| i.total_written);/' crates/hitz-vmm/src/serial_buf.rs
