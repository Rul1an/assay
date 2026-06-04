use assay_runner_schema::{
    ClaimGateDecision, CoverageClaimKind, CoverageCompleteness, CoverageDescriptor, EffectDimension,
};
use serde_json::json;

#[test]
fn filesystem_descriptor_serializes_blindspots_as_data() {
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();

    assert_eq!(descriptor.schema, "assay.runner.coverage_descriptor.v0");
    assert_eq!(descriptor.dimension, EffectDimension::Filesystem);
    assert_eq!(
        descriptor.completeness,
        CoverageCompleteness::OpenSyscallOnly
    );
    assert!(descriptor
        .known_blind_spots
        .iter()
        .any(|blindspot| blindspot.contains("io_uring")));

    let value = serde_json::to_value(&descriptor).expect("descriptor serializes");
    assert_eq!(
        value,
        json!({
            "schema": "assay.runner.coverage_descriptor.v0",
            "dimension": "filesystem",
            "method": "open/openat/openat2 tracepoints",
            "observes": ["path opens through syscall tracepoints"],
            "known_blind_spots": [
                "io_uring file operations may bypass syscall tracepoints",
                "mmap-backed writes are not path-open observations"
            ],
            "completeness": "open_syscall_only"
        })
    );
}

#[test]
fn partial_filesystem_coverage_allows_positive_but_blocks_absence() {
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();

    assert_eq!(
        descriptor
            .claim_decision(CoverageClaimKind::PositiveExistence)
            .decision,
        ClaimGateDecision::Allowed
    );
    assert_eq!(
        descriptor
            .claim_decision(CoverageClaimKind::ExhaustiveSet)
            .decision,
        ClaimGateDecision::Degraded
    );

    let bounded_negative = descriptor.claim_decision(CoverageClaimKind::BoundedNegative);
    assert_eq!(bounded_negative.decision, ClaimGateDecision::Blocked);
    assert_eq!(
        bounded_negative.rule,
        "coverage_descriptor_blocks_absence_claim"
    );
    assert!(bounded_negative.reason.contains("io_uring"));
}

#[test]
fn missing_descriptor_blocks_all_claim_kinds() {
    assert_eq!(
        CoverageDescriptor::claim_decision_for(None, CoverageClaimKind::PositiveExistence).decision,
        ClaimGateDecision::Blocked
    );
    assert_eq!(
        CoverageDescriptor::claim_decision_for(None, CoverageClaimKind::ExhaustiveSet).decision,
        ClaimGateDecision::Blocked
    );
    assert_eq!(
        CoverageDescriptor::claim_decision_for(None, CoverageClaimKind::BoundedNegative).decision,
        ClaimGateDecision::Blocked
    );
}

#[test]
fn network_connect_only_is_positive_but_not_an_exhaustive_peer_set() {
    let descriptor = CoverageDescriptor::network_connect_only();

    assert_eq!(descriptor.dimension, EffectDimension::Network);
    assert_eq!(descriptor.completeness, CoverageCompleteness::ConnectOnly);
    assert_eq!(
        descriptor
            .claim_decision(CoverageClaimKind::PositiveExistence)
            .decision,
        ClaimGateDecision::Allowed
    );

    let exhaustive = descriptor.claim_decision(CoverageClaimKind::ExhaustiveSet);
    assert_eq!(exhaustive.decision, ClaimGateDecision::Degraded);
    assert!(exhaustive.reason.contains("QUIC"));
}
