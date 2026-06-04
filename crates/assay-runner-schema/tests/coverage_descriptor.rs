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
fn malformed_descriptor_schema_blocks_claim_decisions() {
    let mut descriptor = CoverageDescriptor::filesystem_open_syscall_only();
    descriptor.schema = "assay.runner.coverage_descriptor.v_next".to_string();

    let decision = CoverageDescriptor::claim_decision_for(
        Some(&descriptor),
        CoverageClaimKind::PositiveExistence,
    );

    assert_eq!(decision.decision, ClaimGateDecision::Blocked);
    assert_eq!(decision.rule, "coverage_descriptor_schema_mismatch");
}

#[test]
fn non_full_completeness_blocks_absence_even_without_blindspot_text() {
    let mut descriptor = CoverageDescriptor::filesystem_open_syscall_only();
    descriptor.known_blind_spots.clear();

    let exhaustive = descriptor.claim_decision(CoverageClaimKind::ExhaustiveSet);
    assert_eq!(exhaustive.decision, ClaimGateDecision::Degraded);
    assert_eq!(
        exhaustive.rule,
        "coverage_descriptor_degrades_exhaustive_claim"
    );

    let bounded_negative = descriptor.claim_decision(CoverageClaimKind::BoundedNegative);
    assert_eq!(bounded_negative.decision, ClaimGateDecision::Blocked);
    assert_eq!(
        bounded_negative.rule,
        "coverage_descriptor_blocks_absence_claim"
    );
    assert!(bounded_negative.reason.contains("open_syscall_only"));
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

#[test]
fn observes_effect_class_matches_case_insensitive_substring() {
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();
    assert!(descriptor.observes_effect_class("path opens"));
    assert!(descriptor.observes_effect_class("PATH OPENS"));
    assert!(!descriptor.observes_effect_class("network connect"));
    assert!(!descriptor.observes_effect_class("   "));
}

#[test]
fn positive_claim_for_observed_class_is_allowed() {
    let descriptor = CoverageDescriptor::network_connect_only();
    let decision = CoverageDescriptor::claim_decision_for_effect(
        Some(&descriptor),
        CoverageClaimKind::PositiveExistence,
        "connect-time peer endpoints",
    );
    assert_eq!(decision.decision, ClaimGateDecision::Allowed);
    assert_eq!(
        decision.rule,
        "coverage_descriptor_allows_observed_positive_claim"
    );
}

#[test]
fn positive_claim_for_unobserved_class_is_degraded() {
    let descriptor = CoverageDescriptor::network_connect_only();
    // connect-only capture does not observe post-connect datagram peers
    let decision = CoverageDescriptor::claim_decision_for_effect(
        Some(&descriptor),
        CoverageClaimKind::PositiveExistence,
        "datagram peer after connect",
    );
    assert_eq!(decision.decision, ClaimGateDecision::Degraded);
    assert_eq!(
        decision.rule,
        "coverage_descriptor_positive_class_not_observed"
    );
    assert!(decision.reason.contains("datagram peer after connect"));
}

#[test]
fn effect_class_variant_leaves_exhaustive_and_bounded_unchanged() {
    let descriptor = CoverageDescriptor::filesystem_open_syscall_only();
    // class argument is irrelevant for non-positive claim kinds
    let exhaustive = CoverageDescriptor::claim_decision_for_effect(
        Some(&descriptor),
        CoverageClaimKind::ExhaustiveSet,
        "anything",
    );
    assert_eq!(exhaustive.decision, ClaimGateDecision::Degraded);
    assert_eq!(
        exhaustive.rule,
        "coverage_descriptor_degrades_exhaustive_claim"
    );

    let bounded = CoverageDescriptor::claim_decision_for_effect(
        Some(&descriptor),
        CoverageClaimKind::BoundedNegative,
        "anything",
    );
    assert_eq!(bounded.decision, ClaimGateDecision::Blocked);
}

#[test]
fn missing_descriptor_still_blocks_effect_variant() {
    let decision = CoverageDescriptor::claim_decision_for_effect(
        None,
        CoverageClaimKind::PositiveExistence,
        "path opens",
    );
    assert_eq!(decision.decision, ClaimGateDecision::Blocked);
    assert_eq!(decision.rule, "coverage_descriptor_required_for_claim");
}
