# hitz-api

`hitz-api` provides the core serializable data structures and communication vocabulary for the Hitz micro-VM manager.

## Overview

This crate defines the REST API payload structures (JSON over HTTP) and the vsock telemetry structures (MessagePack over vsock) used for communication between:
1. The `hitz-cli` command-line tool.
2. The `hitz-daemon` background service.
3. The `hitz-guest-agent` running inside the micro-VM.

By keeping these structures strictly defined here, we ensure ABI stability and safe network transmission across all boundaries.

## Features

- **Health Check (`health_check`):** Evaluates real-time VM telemetry to determine system health.
- **Diff (`diff`):** Calculates rates of change (like network bytes/sec) between two telemetry snapshots.
- **Prometheus (`prometheus`):** Exports VM metrics to standard Prometheus text exposition format.
- **Carbon Estimator (`carbon`):** Estimates real-time CO2 emissions based on grid intensity and hardware utilization.
- **Right Sizer (`rightsizer`):** Recommends VM configuration scaling based on historical usage.
- **Terraform (`terraform`):** Generates Terraform HCL representation of VM configurations.

## Architecture

This is a pure data crate. It performs no network I/O, does not spawn threads, and has minimal dependencies, ensuring it can easily compile to lightweight targets (like `x86_64-unknown-linux-musl` for the guest agent).
