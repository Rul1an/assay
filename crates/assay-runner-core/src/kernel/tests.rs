use super::*;
use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_CONNECT_BLOCKED, EVENT_EXEC, EVENT_FILE_BLOCKED,
    EVENT_INODE_RESOLVED, EVENT_OPENAT, EVENT_SENDMSG, EVENT_SENDTO,
};
use assay_monitor::MonitorStatsSnapshot;

fn event(event_type: u32, value: &[u8]) -> MonitorEvent {
    let mut event = MonitorEvent::zeroed();
    event.pid = 42;
    event.event_type = event_type;
    event.data[..value.len()].copy_from_slice(value);
    event
}

fn open_event(value: &[u8], flags: u64, return_value: i64) -> MonitorEvent {
    let mut event = event(EVENT_OPENAT, value);
    event.flags = flags;
    event.mode = 0o644;
    event.return_value = return_value;
    event
}

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
fn datagram_peer_stats_upgrade_network_protocol_coverage() {
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        connect_events_emitted: 1,
        sendmsg_events_emitted: 1,
        ..Default::default()
    };
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/known\0"))
        .unwrap();
    let capture = builder.finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::DiagnosticOnly
    );
    assert!(archive.observation_health.notes.iter().any(|note| {
        note.contains("network_protocol_coverage=connect_and_datagram_peer_observed")
    }));
}

#[test]
fn datagram_only_stats_mark_datagram_peer_observed() {
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        sendto_events_emitted: 1,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::DatagramPeerObserved
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::DiagnosticOnly
    );
    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("network_protocol_coverage=datagram_peer_observed")));
}

#[test]
fn send_no_recoverable_peer_count_surfaces_in_note() {
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        sendto_no_peer: 2,
        sendmsg_no_peer: 1,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("send_no_recoverable_peer=sendto:2 sendmsg:1")));
}

#[test]
fn no_recoverable_peer_sends_do_not_upgrade_network_protocol_coverage() {
    // Address-less sends must NOT claim a datagram peer was observed — the
    // peer is unrecoverable and the socket type is unknown. Coverage stays.
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        sendto_no_peer: 5,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_ne!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::DatagramPeerObserved
    );
    assert_ne!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved
    );
}

#[test]
fn zero_no_peer_count_leaves_note_byte_identical() {
    // The invariant: a run with no address-less sends must not gain the
    // suffix, so existing clean archives read identically.
    let snap = MonitorStatsSnapshot::default();
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&snap, &snap);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert!(archive
        .observation_health
        .notes
        .iter()
        .all(|note| !note.contains("send_no_recoverable_peer")));
}

#[test]
fn send_non_ip_family_count_surfaces_in_note() {
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        sendto_non_ip_family: 4,
        sendmsg_non_ip_family: 2,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("send_non_ip_family=sendto:4 sendmsg:2")));
}

#[test]
fn non_ip_family_sends_do_not_upgrade_network_protocol_coverage() {
    // A non-IP send (e.g. AF_UNIX) is not an IP peer and must not claim one.
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        sendto_non_ip_family: 9,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_ne!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::DatagramPeerObserved
    );
    assert_ne!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved
    );
}

#[test]
fn zero_non_ip_family_count_leaves_note_byte_identical() {
    let snap = MonitorStatsSnapshot::default();
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&snap, &snap);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert!(archive
        .observation_health
        .notes
        .iter()
        .all(|note| !note.contains("send_non_ip_family")));
}

#[test]
fn ringbuf_drop_delta_marks_partial_health_when_applied() {
    let before = MonitorStatsSnapshot {
        tracepoint_ringbuf_dropped: 2,
        ..Default::default()
    };
    let after = MonitorStatsSnapshot {
        tracepoint_ringbuf_dropped: 5,
        lsm_ringbuf_dropped: 1,
        ..Default::default()
    };
    let builder = KernelLayerBuilder::new("run_001").unwrap();
    let capture = builder.finish(&before, &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_eq!(
        archive.observation_health.kernel_layer,
        KernelLayerStatus::PartialRingbufDrops
    );
    assert_eq!(archive.observation_health.ringbuf_drops, 4);
    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::ConnectOnly
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::DiagnosticOnly
    );
    archive.observation_health.validate().unwrap();
}

#[test]
fn clean_capture_can_mark_kernel_complete() {
    let mut builder = KernelLayerBuilder::new("run_001").unwrap();
    builder
        .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/known\0"))
        .unwrap();
    let capture = builder.finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_eq!(
        archive.observation_health.kernel_layer,
        KernelLayerStatus::Complete
    );
    assert_eq!(
        archive.observation_health.cgroup_correlation,
        CgroupCorrelationStatus::Clean
    );
    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::ConnectOnly
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::DiagnosticOnly
    );
    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("network_protocol_coverage=connect_only")));
}

#[test]
fn partial_cgroup_correlation_downgrades_kernel_layer_to_absent() {
    let capture = KernelLayerBuilder::new("run_001").unwrap().finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Partial)
        .unwrap();

    assert_eq!(
        archive.observation_health.kernel_layer,
        KernelLayerStatus::Absent
    );
    assert_eq!(
        archive.observation_health.cgroup_correlation,
        CgroupCorrelationStatus::Partial
    );
    assert_eq!(archive.observation_health.ringbuf_drops, 0);
    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::Absent
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::NotApplicable
    );
    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("kernel_layer downgraded to absent")));
    archive.observation_health.validate().unwrap();
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
fn apply_rejects_run_id_mismatch() {
    let capture = KernelLayerBuilder::new("run_001").unwrap().finish(
        &MonitorStatsSnapshot::default(),
        &MonitorStatsSnapshot::default(),
    );
    let mut archive = RunnerSpikeArchive::empty("run_002", "linux");

    let err = capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap_err();

    assert!(matches!(err, KernelLayerError::RunIdMismatch { .. }));
}

#[test]
fn apply_on_non_linux_keeps_kernel_absent() {
    let after = MonitorStatsSnapshot {
        tracepoint_ringbuf_dropped: 2,
        ..Default::default()
    };
    let capture = KernelLayerBuilder::new("run_001")
        .unwrap()
        .finish(&MonitorStatsSnapshot::default(), &after);
    let mut archive = RunnerSpikeArchive::empty("run_001", "macos");

    capture
        .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
        .unwrap();

    assert_eq!(
        archive.observation_health.kernel_layer,
        KernelLayerStatus::Absent
    );
    assert_eq!(
        archive.observation_health.cgroup_correlation,
        CgroupCorrelationStatus::Partial
    );
    assert_eq!(archive.observation_health.ringbuf_drops, 0);
    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::Absent
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::NotApplicable
    );
    archive.observation_health.validate().unwrap();
}
