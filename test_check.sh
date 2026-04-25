#!/bin/bash
# Check if windows-only stuff causes the build to fail on linux when we just want to run tests for hitz-cli.
# hitz-cli appears to only have a binary, not a lib.
cargo check -p hitz-cli --bin hitz --target x86_64-unknown-linux-gnu 2>&1 | head -n 30
