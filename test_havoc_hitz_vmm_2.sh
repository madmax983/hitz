#!/bin/bash
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test -p hitz-vmm --target x86_64-pc-windows-gnu -- havoc
