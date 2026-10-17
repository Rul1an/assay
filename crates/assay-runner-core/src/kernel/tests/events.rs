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
    let mut socket_payload = [0_u8; 512];
    socket_payload[0..8].copy_from_slice(&99_u64.to_ne_bytes());
    socket_payload[8..10].copy_from_slice(&2_u16.to_ne_bytes());
    socket_payload[10..12].copy_from_slice(&443_u16.to_ne_bytes());
    socket_payload[12..16].copy_from_slice(&[10, 0, 0, 5]);
    socket_payload[32..36].copy_from_slice(&17_u32.to_ne_bytes());
    socket_payload[36..40].copy_from_slice(&2_u32.to_ne_bytes());
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    builder
        .push_monitor_event(&event(EVENT_CONNECT_BLOCKED, &socket_payload))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let record: KernelLayerEvent = serde_json::from_slice(&capture.kernel_layer_ndjson).unwrap();

    assert_eq!(record.kind, "connect_blocked");
    assert_eq!(record.value.as_deref(), Some("10.0.0.5:443"));
    assert_eq!(record.cgroup_id, Some(99));
    assert_eq!(record.network_destination.as_deref(), Some("10.0.0.5"));
    assert_eq!(record.network_port, Some(443));
    assert_eq!(record.rule_id, Some(17));
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

#[test]
fn admitted_records_up_to_byte_cap_preserve_wire_ndjson_and_ignore_inode_telemetry() {
    let mut golden_builder = KernelLayerBuilder::new("run_001").unwrap();
    golden_builder
        .push_monitor_event(&event(EVENT_INODE_RESOLVED, &[1, 2, 3, 4]))
        .unwrap();
    golden_builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/true\0"))
        .unwrap();
    golden_builder
        .push_monitor_event(&open_event(b"/tmp/assay-known-file\0", 0, 3))
        .unwrap();
    let golden_capture = golden_builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    assert_eq!(
        String::from_utf8(golden_capture.kernel_layer_ndjson).unwrap(),
        concat!(
            "{\"schema\":\"assay.runner.kernel_event.v0\",\"run_id\":\"run_001\",\"seq\":0,\"pid\":42,\"event_type\":4,\"kind\":\"exec\",\"value\":\"/usr/bin/true\"}\n",
            "{\"schema\":\"assay.runner.kernel_event.v0\",\"run_id\":\"run_001\",\"seq\":1,\"pid\":42,\"event_type\":1,\"kind\":\"openat\",\"value\":\"/tmp/assay-known-file\",\"flags\":0,\"mode\":420,\"return_value\":3,\"access_mode\":\"read\",\"status\":\"success\"}\n",
        )
    );

    let run_id = four_event_byte_cap_run_id();
    let mut builder = KernelLayerBuilder::new(run_id.clone()).unwrap();
    let mut expected_ndjson = Vec::with_capacity(TEST_MAX_NDJSON_BYTES);

    for seq in 0..4_u64 {
        builder
            .push_monitor_event(&event(EVENT_INODE_RESOLVED, &[1, 2, 3, 4]))
            .unwrap();
        builder
            .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/true\0"))
            .unwrap();
        expected_ndjson.extend_from_slice(
            format!(
                "{{\"schema\":\"assay.runner.kernel_event.v0\",\"run_id\":\"{run_id}\",\"seq\":{seq},\"pid\":42,\"event_type\":4,\"kind\":\"exec\",\"value\":\"/usr/bin/true\"}}\n"
            )
            .as_bytes(),
        );
    }
    builder
        .push_monitor_event(&event(EVENT_INODE_RESOLVED, &[9, 9, 9, 9]))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 4);
    assert_eq!(capture.kernel_layer_ndjson.len(), TEST_MAX_NDJSON_BYTES);
    assert_eq!(capture.kernel_layer_ndjson, expected_ndjson);
    assert!(capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/true"));
}

#[test]
fn record_limit_refuses_10001st_event_leaves_prefix_unchanged_and_sticks() {
    let mut prefix_builder = KernelLayerBuilder::new("run_001").unwrap();
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    for _ in 0..TEST_MAX_RECORDS {
        let ev = event(EVENT_OPENAT, b"/tmp/assay-admitted\0");
        prefix_builder.push_monitor_event(&ev).unwrap();
        builder.push_monitor_event(&ev).unwrap();
    }
    let filtered = event(EVENT_OPENAT, b"/lib/libc.so.6\0");
    prefix_builder.push_monitor_event(&filtered).unwrap();
    builder.push_monitor_event(&filtered).unwrap();
    let inode = event(EVENT_INODE_RESOLVED, &[1, 2, 3, 4]);
    prefix_builder.push_monitor_event(&inode).unwrap();
    builder.push_monitor_event(&inode).unwrap();

    let prefix_capture = prefix_builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    // 10,001st admitted record crosses the record cap; push still returns Ok(()).
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/assay-over-record-cap\0"))
        .unwrap();
    // Subsequent pushes (admitted, filtered, and ignored inode) are sticky no-ops.
    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/after-record-cap\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libc.so.6\0"))
        .unwrap();
    builder.push_monitor_event(&inode).unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, TEST_MAX_RECORDS);
    assert_eq!(
        capture.kernel_layer_ndjson,
        prefix_capture.kernel_layer_ndjson
    );
    assert_eq!(
        capture.capability_surface,
        prefix_capture.capability_surface
    );
    assert!(!capture
        .capability_surface
        .filesystem_paths
        .contains("/tmp/assay-over-record-cap"));
    assert!(!capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/after-record-cap"));
    assert_eq!(
        capture.filtering_layers(),
        prefix_capture.filtering_layers()
    );
}

#[test]
fn ndjson_byte_limit_includes_trailing_lf_leaves_prefix_unchanged_and_sticks() {
    let line_len = TEST_MAX_NDJSON_BYTES / 4;
    let run_id = four_event_byte_cap_run_id();
    let mut prefix_builder = KernelLayerBuilder::new(run_id.clone()).unwrap();
    let mut builder = KernelLayerBuilder::new(run_id).unwrap();

    for _ in 0..3 {
        let ev = event(EVENT_EXEC, b"/usr/bin/true\0");
        prefix_builder.push_monitor_event(&ev).unwrap();
        builder.push_monitor_event(&ev).unwrap();
    }
    let prefix_capture = prefix_builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    assert_eq!(prefix_capture.event_count, 3);
    assert_eq!(prefix_capture.kernel_layer_ndjson.len(), 3 * line_len);

    // 4th event has JSON body == line_len (4 MiB total without LF) and line_len + 1 with LF.
    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/true!\0"))
        .unwrap();
    // Subsequent pushes must remain sticky no-ops even for filtered or smaller events.
    builder
        .push_monitor_event(&event(EVENT_FILE_BLOCKED, b"/etc/after-byte-cap\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libc.so.6\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 3);
    assert_eq!(
        capture.kernel_layer_ndjson,
        prefix_capture.kernel_layer_ndjson
    );
    assert_eq!(
        capture.capability_surface,
        prefix_capture.capability_surface
    );
    assert!(!capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/true!"));
    assert!(!capture
        .capability_surface
        .filesystem_paths
        .contains("/etc/after-byte-cap"));
    assert_eq!(
        capture.filtering_layers(),
        prefix_capture.filtering_layers()
    );
}

#[test]
fn ndjson_byte_limit_catches_json_escape_expansion_and_leaves_prefix_unchanged() {
    let run_id = four_event_byte_cap_run_id();
    let mut prefix_builder = KernelLayerBuilder::new(run_id.clone()).unwrap();
    let mut builder = KernelLayerBuilder::new(run_id.clone()).unwrap();

    for _ in 0..3 {
        let ev = event(EVENT_EXEC, b"/usr/bin/true\0");
        prefix_builder.push_monitor_event(&ev).unwrap();
        builder.push_monitor_event(&ev).unwrap();
    }
    let prefix_capture = prefix_builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    // Raw path "/usr/bin/\x01\x02" is 11 bytes (2 bytes shorter than "/usr/bin/true"),
    // but serde_json escapes \x01\x02 to \u0001\u0002 (21 JSON bytes), overflowing 4 MiB.
    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/\x01\x02\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_FILE_BLOCKED, b"/etc/after-escape-overflow\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 3);
    assert_eq!(
        capture.kernel_layer_ndjson,
        prefix_capture.kernel_layer_ndjson
    );
    assert_eq!(
        capture.capability_surface,
        prefix_capture.capability_surface
    );
    assert!(!capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/\x01\x02"));
    assert!(!capture
        .capability_surface
        .filesystem_paths
        .contains("/etc/after-escape-overflow"));

    let mut archive = sentinel_archive(&run_id);
    let expected_archive = archive.clone();
    let err = capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap_err();
    assert!(!matches!(err, KernelLayerError::RunIdMismatch { .. }));
    assert_eq!(archive, expected_archive);
}

#[test]
fn ndjson_byte_limit_prechecks_oversized_run_id_on_first_push() {
    let run_id = "r".repeat(TEST_MAX_NDJSON_BYTES);
    let mut builder = KernelLayerBuilder::new(run_id).unwrap();

    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/oversized-run-id\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libc.so.6\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 0);
    assert!(capture.kernel_layer_ndjson.is_empty());
    assert!(capture.capability_surface.filesystem_paths.is_empty());
    assert_eq!(capture.filtering_layers().0, 0);
}

#[test]
fn filtered_loader_path_limit_allows_1024_distinct_with_repeats_and_refuses_1025th_stickily() {
    let mut prefix_builder = KernelLayerBuilder::new("run_001").unwrap();
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    let initial = event(EVENT_OPENAT, b"/tmp/before-filter-cap\0");
    prefix_builder.push_monitor_event(&initial).unwrap();
    builder.push_monitor_event(&initial).unwrap();

    for idx in 0..TEST_MAX_FILTERED_LOADER_PATHS {
        let raw = format!("/lib/libcap_{idx}.so\0");
        let ev = event(EVENT_OPENAT, raw.as_bytes());
        prefix_builder.push_monitor_event(&ev).unwrap();
        builder.push_monitor_event(&ev).unwrap();
    }
    // Repeating already-admitted filtered loader paths at the 1024-key cap is allowed.
    for repeated in ["/lib/libcap_0.so\0", "/lib/libcap_1023.so\0"] {
        let ev = event(EVENT_OPENAT, repeated.as_bytes());
        prefix_builder.push_monitor_event(&ev).unwrap();
        builder.push_monitor_event(&ev).unwrap();
    }

    let prefix_capture = prefix_builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    assert_eq!(
        prefix_capture.filtering_layers().0,
        (TEST_MAX_FILTERED_LOADER_PATHS as u64) + 2
    );

    // 1025th distinct filtered loader path crosses the map-key cap; push returns Ok(()).
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libcap_1024.so\0"))
        .unwrap();
    // Subsequent repeated filtered paths and normal events must be sticky no-ops.
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libcap_0.so\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/after-filter-cap\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/after-filter-cap\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );

    assert_eq!(capture.event_count, 1);
    assert_eq!(
        capture.kernel_layer_ndjson,
        prefix_capture.kernel_layer_ndjson
    );
    assert_eq!(
        capture.capability_surface,
        prefix_capture.capability_surface
    );
    assert!(!capture
        .capability_surface
        .filesystem_paths
        .contains("/tmp/after-filter-cap"));
    assert!(!capture
        .capability_surface
        .process_execs
        .contains("/usr/bin/after-filter-cap"));
    assert_eq!(
        capture.filtering_layers(),
        prefix_capture.filtering_layers()
    );
}

#[test]
fn filtered_loader_repeated_path_counters_saturate_at_u64_max() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();

    for idx in 0..TEST_MAX_FILTERED_LOADER_PATHS {
        let raw = format!("/lib/libcap_{idx}.so\0");
        builder
            .push_monitor_event(&event(EVENT_OPENAT, raw.as_bytes()))
            .unwrap();
    }
    builder.filtered_loader_events = u64::MAX - 1;
    *builder
        .filtered_loader_values
        .get_mut("/lib/libcap_0.so")
        .unwrap() = u64::MAX - 1;

    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libcap_0.so\0"))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/lib/libcap_0.so\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let (filtered_total, retained_total, top_filtered) = capture.filtering_layers();

    assert_eq!(filtered_total, u64::MAX);
    assert_eq!(retained_total, 0);
    assert_eq!(
        top_filtered.first(),
        Some(&("/lib/libcap_0.so".to_string(), u64::MAX))
    );

    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();
}

#[test]
fn kernel_layer_builder_seven_emitted_variants_pass_shared_schema_validator() {
    use assay_runner_schema::{
        is_known_kernel_event_key, EffectDimension, REQUIRED_KERNEL_EVENT_KEYS,
    };

    let mut builder = KernelLayerBuilder::new("run_parity_01").unwrap();

    // 1. openat with operation flags + resolve + success, and openat with error return_value
    let mut open_created = open_event(
        b"/tmp/assay-parity-created\0",
        0o2 | 0o100 | 0o1000 | 0o2000 | 0o400,
        5,
    );
    open_created.resolve = 4;
    builder.push_monitor_event(&open_created).unwrap();
    builder
        .push_monitor_event(&open_event(b"/tmp/assay-parity-missing\0", 0, -2))
        .unwrap();

    // 2. file_blocked
    builder
        .push_monitor_event(&event(EVENT_FILE_BLOCKED, b"/etc/shadow\0"))
        .unwrap();

    // 3. connect (valid IPv4 sockaddr and fallback null sockaddr)
    let mut sockaddr_v4 = [0_u8; 16];
    sockaddr_v4[0..2].copy_from_slice(&2_u16.to_ne_bytes());
    sockaddr_v4[2..4].copy_from_slice(&443_u16.to_be_bytes());
    sockaddr_v4[4..8].copy_from_slice(&[93, 184, 216, 34]);
    builder
        .push_monitor_event(&event(EVENT_CONNECT, &sockaddr_v4))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_CONNECT, &[0, 0]))
        .unwrap();

    // 4. sendto
    builder
        .push_monitor_event(&event(EVENT_SENDTO, &sockaddr_v4))
        .unwrap();

    // 5. sendmsg
    builder
        .push_monitor_event(&event(EVENT_SENDMSG, &sockaddr_v4))
        .unwrap();

    // 6. connect_blocked (decoded 512-byte socket payload and fallback plain sockaddr)
    let mut socket_payload = [0_u8; 512];
    socket_payload[0..8].copy_from_slice(&4242_u64.to_ne_bytes());
    socket_payload[8..10].copy_from_slice(&2_u16.to_ne_bytes());
    socket_payload[10..12].copy_from_slice(&8443_u16.to_ne_bytes());
    socket_payload[12..16].copy_from_slice(&[198, 51, 100, 7]);
    socket_payload[32..36].copy_from_slice(&9_u32.to_ne_bytes());
    socket_payload[36..40].copy_from_slice(&2_u32.to_ne_bytes());
    builder
        .push_monitor_event(&event(EVENT_CONNECT_BLOCKED, &socket_payload))
        .unwrap();
    builder
        .push_monitor_event(&event(EVENT_CONNECT_BLOCKED, &[0, 0]))
        .unwrap();

    // 7. exec
    builder
        .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/env\0"))
        .unwrap();

    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let ndjson = std::str::from_utf8(&capture.kernel_layer_ndjson).unwrap();
    let lines: Vec<&str> = ndjson.lines().collect();
    assert_eq!(lines.len(), 10);

    let expected_dimensions = [
        EffectDimension::Filesystem,
        EffectDimension::Filesystem,
        EffectDimension::Filesystem,
        EffectDimension::Network,
        EffectDimension::Network,
        EffectDimension::Network,
        EffectDimension::Network,
        EffectDimension::Network,
        EffectDimension::Network,
        EffectDimension::Process,
    ];

    for (line, expected_dim) in lines.into_iter().zip(expected_dimensions) {
        let raw_val: serde_json::Value = serde_json::from_str(line).unwrap();
        let obj = raw_val.as_object().unwrap();
        assert!(obj.keys().all(|k| is_known_kernel_event_key(k)));
        assert!(REQUIRED_KERNEL_EVENT_KEYS
            .iter()
            .all(|required| obj.contains_key(*required)));
        assert!(obj.iter().all(|(k, v)| k == "value" || !v.is_null()));
        assert!(!obj
            .get("operation_flags")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty));

        let record: KernelLayerEvent = serde_json::from_value(raw_val).unwrap();
        assert_eq!(record.validate_emitted(), Ok(expected_dim));
    }
}
