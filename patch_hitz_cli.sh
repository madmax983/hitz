#!/bin/bash
sed -i 's/make_bar(snap.cpu.total_pct, 15)/make_bar(snap.cpu.total_pct.into(), 15)/g' crates/hitz-cli/src/main.rs
sed -i 's/color_for_pct(snap.cpu.total_pct)/color_for_pct(snap.cpu.total_pct.into())/g' crates/hitz-cli/src/main.rs
sed -i 's/color_for_pct(proc.cpu_pct)/color_for_pct(proc.cpu_pct.into())/g' crates/hitz-cli/src/main.rs
sed -i 's/err.as_str()/err.to_string()/g' crates/hitz-cli/src/main.rs
