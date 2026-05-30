#[cfg(test)]
#[allow(
    unsafe_code,
    clippy::items_after_statements,
    clippy::ignore_without_reason
)]
mod tests {
    use super::*;

    /// Mutex that serialises any test touching `OTEL_EXPORTER_OTLP_ENDPOINT`
    /// so parallel test threads cannot race on environment state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_cli_wins_over_env() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let cli_val = "http://cli-endpoint:4317".to_string();
        let env_val = "http://env-endpoint:4317";
        // SAFETY: single-threaded test, no other env manipulation concurrent
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", env_val);
        }
        let resolved = resolve_otlp_endpoint(Some(cli_val.clone()));
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        assert_eq!(resolved, Some(cli_val));
    }

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_falls_back_to_env() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let env_val = "http://env-endpoint:4317".to_string();
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", &env_val);
        }
        let resolved = resolve_otlp_endpoint(None);
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        assert_eq!(resolved, Some(env_val));
    }

    #[test]
    #[allow(unsafe_code)]
    fn endpoint_resolution_none_when_absent() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        let resolved = resolve_otlp_endpoint(None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn parse_port_forward_valid() {
        let pf = parse_port_forward("2222:22").expect("valid");
        assert_eq!(pf.host_port, 2222);
        assert_eq!(pf.guest_port, 22);
    }

    #[test]
    fn parse_port_forward_missing_colon() {
        assert!(parse_port_forward("2222").is_err());
    }

    #[test]
    fn parse_port_forward_bad_host() {
        assert!(parse_port_forward("abc:22").is_err());
    }

    #[test]
    fn parse_port_forward_bad_guest() {
        assert!(parse_port_forward("2222:xyz").is_err());
    }

    #[test]
    fn build_launch_args_round_trip() {
        let original = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: r"\\.\pipe\hitz-test".to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch_args = build_launch_args(&original.start).expect("valid UTF-8 path");
        // SCM-only fields must NOT be forwarded to the daemon process.
        assert!(
            !launch_args.iter().any(|a| a == "--auto-start"),
            "--auto-start must not be forwarded; it is SCM-only"
        );
        assert!(
            !launch_args.iter().any(|a| a == "--display-name"),
            "--display-name must not be forwarded; it is SCM-only"
        );
        assert!(
            !launch_args.iter().any(|a| a == "--description"),
            "--description must not be forwarded; it is SCM-only"
        );
        let mut argv = vec![std::ffi::OsString::from("hitz")];
        argv.extend(launch_args.into_iter().map(std::ffi::OsString::from));
        let cli = Cli::try_parse_from(argv).expect("re-parse failed");
        let Command::Daemon(DaemonCommand::Start(recovered)) = cli.command else {
            panic!("expected daemon start");
        };
        assert_eq!(recovered.pipe, original.start.pipe);
        assert_eq!(recovered.state_dir, original.start.state_dir);
        assert_eq!(recovered.verbose, original.start.verbose);
    }

    #[test]
    fn build_launch_args_includes_pipe_and_state_dir() {
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: r"\\.\pipe\custom".to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"D:\vms")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args.start).expect("valid UTF-8 path");
        assert!(launch.contains(&"--pipe".to_string()));
        assert!(launch.contains(&r"\\.\pipe\custom".to_string()));
        assert!(launch.contains(&"--state-dir".to_string()));
        assert!(launch.contains(&r"D:\vms".to_string()));
    }

    #[test]
    fn build_launch_args_verbose_flag() {
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: true,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        let launch = build_launch_args(&args.start).expect("valid UTF-8 path");
        assert!(launch.contains(&"--verbose".to_string()));
    }

    /// Requires admin privileges and `HITZ_TEST_SERVICE=1` env var.
    /// Run: `cargo test -p hitz-cli svc_ -- --ignored --test-threads=1`
    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_install_and_remove() {
        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        // Clean up any leftover from a previous run.
        let _ = remove_service();

        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms-test")),
            },
            auto_start: false,
            display_name: Some("Hitz test service".to_string()),
            description: Some("Integration test".to_string()),
        };
        install_service(&args).expect("install failed");

        // Verify entry exists in SCM.
        let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .expect("open SCM");
        let svc = mgr.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS);
        assert!(svc.is_ok(), "service not found after install");

        remove_service().expect("remove failed");

        // Verify entry is gone.
        let svc2 = mgr.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS);
        assert!(svc2.is_err(), "service still exists after remove");
    }

    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_install_idempotent_error() {
        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        let _ = remove_service();
        let args = DaemonInstallArgs {
            start: DaemonStartArgs {
                pipe: DEFAULT_PIPE.to_string(),
                tcp_listen: None,
                verbose: false,
                otlp_endpoint: None,
                state_dir: Some(PathBuf::from(r"C:\hitz\vms-test")),
            },
            auto_start: false,
            display_name: None,
            description: None,
        };
        install_service(&args).expect("first install failed");
        let result = install_service(&args);
        // Cleanup before asserting so we don't leave the service installed on failure.
        remove_service().expect("cleanup remove failed");
        assert!(result.is_err(), "second install should fail");
    }

    #[test]
    #[ignore = "requires Windows Admin + HITZ_TEST_SERVICE=1"]
    fn svc_remove_nonexistent() {
        if std::env::var("HITZ_TEST_SERVICE").is_err() {
            return;
        }
        let _ = remove_service(); // ensure not installed
        let result = remove_service();
        assert!(
            result.is_err(),
            "remove of non-existent service should error"
        );
    }

    #[test]
    fn metrics_output_formats_snapshot() {
        use hitz_api::{CpuMetrics, DiskMetrics, MemoryMetrics, MetricsSnapshot, NetMetrics};
        let snap = MetricsSnapshot {
            timestamp_ms: 0,
            cpu: CpuMetrics {
                total_pct: 12.5,
                per_core: vec![10.0, 15.0],
                load_avg: [0.42, 0.38, 0.31],
            },
            memory: MemoryMetrics {
                total_bytes: 256 * 1024 * 1024,
                used_bytes: 128 * 1024 * 1024,
                free_bytes: 128 * 1024 * 1024,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: vec![DiskMetrics {
                name: "vda".into(),
                reads_total: 100,
                writes_total: 50,
                read_bytes: 512 * 1024,
                write_bytes: 256 * 1024,
            }],
            networks: vec![NetMetrics {
                interface: "eth0".into(),
                rx_bytes: 1_048_576,
                tx_bytes: 524_288,
                rx_packets: 1000,
                tx_packets: 500,
                rx_errors: 0,
                tx_errors: 0,
            }],
            processes: vec![],
        };
        let output = format_metrics_snapshot(&snap);
        assert!(output.contains("12.5%"), "missing CPU pct: {output}");
        assert!(
            output.contains("128 MiB / 256 MiB"),
            "missing memory: {output}"
        );
        assert!(output.contains("vda"), "missing disk: {output}");
        assert!(output.contains("eth0"), "missing network: {output}");
        assert!(
            output.contains("System:"),
            "missing system header: {output}"
        );
        assert!(output.contains("Disks:"), "missing disks header: {output}");
        assert!(
            output.contains("Networks:"),
            "missing networks header: {output}"
        );
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

#[cfg(test)]
mod top_tests {
    use crate::{VmExportArgs, VmIdArgs};
    use std::path::PathBuf;

    // Just a sanity check to verify the compiler parses everything.
    // Testing terminal UI without a real terminal attached can block/panic,
    // so we just assert our command layout exists and compiles correctly.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_top_command_definition_compiles() {
        let _args = VmIdArgs {
            id: "test".to_string(),
            pipe: "pipe".to_string(),
            tcp: None,
        };
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_record_command_definition_compiles() {
        let _args = crate::VmRecordArgs {
            id: "test".to_string(),
            out: PathBuf::from("metrics.jsonl"),
            interval_ms: 1000,
            duration_secs: None,
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_export_metrics_command_definition_compiles() {
        let _args = VmExportArgs {
            id: "test".to_string(),
            out: PathBuf::from("metrics.json"),
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_dashboard_command_definition_compiles() {
        let _args = crate::VmListArgs {
            pipe: "pipe".to_string(),
            tcp: None,
        };
        assert!(true);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn run_vm_timeline_command_definition_compiles() {
        let _args = crate::VmTimelineArgs {
            in_file: PathBuf::from("metrics.jsonl"),
        };
        assert!(true);
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_port_forward() {
        let pf = parse_port_forward("8080:80").unwrap();
        assert_eq!(pf.host_port, 8080);
        assert_eq!(pf.guest_port, 80);

        assert!(parse_port_forward("8080").is_err());
        assert!(parse_port_forward("8080:abc").is_err());
        assert!(parse_port_forward("abc:80").is_err());
        assert!(parse_port_forward("70000:80").is_err()); // > 65535
    }
    #[test]
    fn test_format_error_response_json() {
        let status = hyper::StatusCode::BAD_REQUEST;
        let json_resp = r#"{"message": "Invalid config", "code": 400}"#;
        let result = format_error_response(status, json_resp, "Failed");
        assert!(result.contains("Invalid config"));
    }

    #[test]
    fn test_format_error_response_plain() {
        let status = hyper::StatusCode::NOT_FOUND;
        let result = format_error_response(status, "Not found anywhere", "Failed");
        assert!(result.contains("Not found anywhere"));
    }
}
