use super::*;

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
    // Address-less sends must NOT claim a datagram peer was observed; the
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
