use super::*;

#[test]
fn openat_event_records_filesystem_capability() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/assay-known-file\0"))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(String::from_utf8(capture.kernel_layer_ndjson.clone())
        .unwrap()
        .contains("\"kind\":\"openat\""));
    assert!(capture
        .capability_surface
        .filesystem_paths
        .contains("/tmp/assay-known-file"));
    assert_eq!(capture.ringbuf_drops, 0);
}

#[test]
fn openat_event_records_flags_access_mode_and_return_value() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&open_event(
            b"/tmp/assay-created-file\0",
            0o1 | 0o100 | 0o1000,
            7,
        ))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let record: KernelLayerEvent = serde_json::from_slice(&capture.kernel_layer_ndjson).unwrap();

    assert_eq!(record.flags, Some(0o1 | 0o100 | 0o1000));
    assert_eq!(record.mode, Some(0o644));
    assert_eq!(record.return_value, Some(7));
    assert_eq!(record.access_mode.as_deref(), Some("write"));
    assert_eq!(
        record.operation_flags,
        vec!["create".to_string(), "truncate".to_string()]
    );
    assert_eq!(record.status.as_deref(), Some("success"));
}

#[test]
fn failed_openat_event_records_error_status() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&open_event(b"/tmp/missing\0", 0, -2))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let record: KernelLayerEvent = serde_json::from_slice(&capture.kernel_layer_ndjson).unwrap();

    assert_eq!(record.access_mode.as_deref(), Some("read"));
    assert_eq!(record.return_value, Some(-2));
    assert_eq!(record.status.as_deref(), Some("error"));
}

#[test]
fn openat_loader_telemetry_is_not_runner_spike_evidence() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    for path in [
        "/etc/ld.so.cache",
        "/etc/localtime",
        "/etc/ssl/openssl.cnf",
        "/lib/aarch64-linux-gnu/libc.so.6",
        "/usr/lib/locale/C.UTF-8/LC_IDENTIFICATION",
        "/usr/share/locale/locale.alias",
        "/proc/self/maps",
        "/sys/fs/cgroup/cgroup.controllers",
        "/dev/null",
        "/usr/pyvenv.cfg",
        "/usr/bin/pyvenv.cfg",
        "/usr/bin/python3._pth",
        "/usr/bin/python3.12._pth",
        "/usr/bin/pybuilddir.txt",
        "/opt/actions-runner/_work/assay/assay/runner-fixtures/openai-agents/node_modules/@openai/agents/package.json",
        "/home/github-runner/.rustup/toolchains/stable/lib/libc.so.6",
        "/opt/actions-runner/_work/assay/assay/target/debug/build/ring/out/libc.so.6",
        "/opt/actions-runner/_work/assay/assay/target/debug/deps/libc.so.6",
    ] {
        builder
            .push_monitor_event(&event(EVENT_OPENAT, format!("{path}\0").as_bytes()))
            .unwrap();
    }

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 0);
    assert!(capture.kernel_layer_ndjson.is_empty());
    assert!(capture.capability_surface.filesystem_paths.is_empty());
}

#[test]
fn file_blocked_loader_path_is_preserved_as_policy_evidence() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(
            EVENT_FILE_BLOCKED,
            b"/lib/aarch64-linux-gnu/libc.so.6\0",
        ))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 1);
    assert!(capture
        .capability_surface
        .filesystem_paths
        .contains("/lib/aarch64-linux-gnu/libc.so.6"));
}

#[test]
fn exec_event_records_process_capability() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/true\0"))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/true"));
}

#[test]
fn builder_rejects_unsafe_run_id() {
    assert!(matches!(
        KernelLayerBuilder::new("../bad"),
        Err(KernelLayerError::UnsafeRunId)
    ));
}

#[test]
fn file_blocked_event_records_filesystem_capability() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_FILE_BLOCKED, b"/etc/passwd\0"))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(capture
        .capability_surface
        .filesystem_paths
        .contains("/etc/passwd"));
}

#[test]
fn ipv4_connect_event_records_network_capability() {
    let mut sockaddr = [0_u8; 16];
    sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
    sockaddr[2..4].copy_from_slice(&8080_u16.to_be_bytes());
    sockaddr[4..8].copy_from_slice(&[127, 0, 0, 1]);
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_CONNECT, &sockaddr))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(capture
        .capability_surface
        .network_endpoints
        .contains("127.0.0.1:8080"));
}

#[test]
fn connect_blocked_event_records_network_capability() {
    let mut sockaddr = [0_u8; 16];
    sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
    sockaddr[2..4].copy_from_slice(&443_u16.to_be_bytes());
    sockaddr[4..8].copy_from_slice(&[10, 0, 0, 5]);
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_CONNECT_BLOCKED, &sockaddr))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(capture
        .capability_surface
        .network_endpoints
        .contains("10.0.0.5:443"));
}

#[test]
fn sendto_event_records_datagram_network_capability() {
    let mut sockaddr = [0_u8; 16];
    sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
    sockaddr[2..4].copy_from_slice(&7844_u16.to_be_bytes());
    sockaddr[4..8].copy_from_slice(&[198, 41, 192, 107]);
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_SENDTO, &sockaddr))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let record: KernelLayerEvent = serde_json::from_slice(&capture.kernel_layer_ndjson).unwrap();

    assert_eq!(record.kind, "sendto");
    assert_eq!(record.value.as_deref(), Some("198.41.192.107:7844"));
    assert!(capture
        .capability_surface
        .network_endpoints
        .contains("198.41.192.107:7844"));
}

#[test]
fn sendmsg_event_records_datagram_network_capability() {
    let mut sockaddr = [0_u8; 16];
    sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
    sockaddr[2..4].copy_from_slice(&7844_u16.to_be_bytes());
    sockaddr[4..8].copy_from_slice(&[198, 41, 200, 43]);
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_SENDMSG, &sockaddr))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let record: KernelLayerEvent = serde_json::from_slice(&capture.kernel_layer_ndjson).unwrap();

    assert_eq!(record.kind, "sendmsg");
    assert_eq!(record.value.as_deref(), Some("198.41.200.43:7844"));
    assert!(capture
        .capability_surface
        .network_endpoints
        .contains("198.41.200.43:7844"));
}

#[test]
fn invalid_sockaddr_is_preserved_as_event_without_capability() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_CONNECT, &[0, 0]))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert!(capture.capability_surface.network_endpoints.is_empty());
    assert!(String::from_utf8(capture.kernel_layer_ndjson)
        .unwrap()
        .contains("\"value\":null"));
}

#[test]
fn inode_resolved_telemetry_is_not_runner_spike_evidence() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_INODE_RESOLVED, &[1, 2, 3, 4]))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 0);
    assert!(capture.kernel_layer_ndjson.is_empty());
    assert!(capture.capability_surface.filesystem_paths.is_empty());
    assert!(capture.capability_surface.network_endpoints.is_empty());
    assert!(capture.capability_surface.process_execs.is_empty());
}
