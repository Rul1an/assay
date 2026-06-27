use super::*;

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
        NetworkProtocolCoverageStatus::Unknown
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::Unknown
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
        .any(|note| note.contains("network_protocol_coverage=absent")
            && note.contains("network_endpoint_claim_scope=not_applicable")));
}

#[test]
fn network_hook_drop_without_network_emit_marks_unknown_coverage() {
    let before = MonitorStatsSnapshot::default();
    let after = MonitorStatsSnapshot {
        connect_ringbuf_dropped: 1,
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
        archive.observation_health.kernel_layer,
        KernelLayerStatus::Complete
    );
    assert_eq!(
        archive.observation_health.network_protocol_coverage,
        NetworkProtocolCoverageStatus::Unknown
    );
    assert_eq!(
        archive.observation_health.network_endpoint_claim_scope,
        NetworkEndpointClaimScope::Unknown
    );
    assert!(archive
        .observation_health
        .notes
        .iter()
        .any(|note| note.contains("network_protocol_coverage=unknown")
            && note.contains("network_endpoint_claim_scope=unknown")));
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
