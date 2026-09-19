use assay_evidence::incident_package::{
    verify_incident_package, ContextExpectation, ContextInput, ContextLimits, IncidentExpectation,
    IncidentOutcome, IncidentReason, NON_CLAIMS_DEFAULT,
};

const FIXTURES_DIR: &str = "tests/fixtures/incident_package";

fn read_fixture(name: &str) -> Vec<u8> {
    let path = format!("{FIXTURES_DIR}/{name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn test_valid_present_empty_verifies() {
    let pkg_bytes = read_fixture("valid_present_empty.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
    assert_eq!(report.expectation, IncidentExpectation::NotRequested);
    assert!(report.artifact_sha256.is_some());
    assert_eq!(
        report.inventory_sha256.as_deref(),
        Some("747ae82961390332a1a203d8a6f7d8ffc3d7ecee77825c5c470dca627138e99b")
    );
    assert_eq!(
        report.assessment_sha256.as_deref(),
        Some("921dda0ca89d242f89ed1450748a3934335103cc4fdb34dc731969f53bd7397c")
    );
    assert_eq!(
        report.verification_context_sha256.as_deref(),
        Some("919787b66b4dc0688944d7c76218c6da168890f8251985ccfb12e2a92cfcb814")
    );
    assert_eq!(
        report.non_claims,
        NON_CLAIMS_DEFAULT
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );

    let counts = report.counts.expect("counts present on success");
    assert_eq!(counts["inputs"], 1);
    assert_eq!(counts["objects"], 1);
    assert_eq!(counts["units"], 0);
    assert_eq!(counts["examined"], 0);
    assert_eq!(counts["withheld"], 0);
    assert_eq!(counts["resolved_result_count"], 0);
}

#[test]
fn test_phase_1_refusal_trust_input() {
    let pkg_bytes = read_fixture("valid_present_empty.tar");

    // Invalid schema pin
    let ctx = ContextInput {
        schema_pin: "0".repeat(64),
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));

    // Invalid limit (zero outer bytes)
    let ctx = ContextInput {
        limits: ContextLimits {
            outer_bytes: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));

    // Invalid limit (> hard limit)
    let ctx = ContextInput {
        limits: ContextLimits {
            outer_bytes: ContextLimits::HARD.outer_bytes + 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));

    // Invalid timestamp interval: valid_from > valid_until
    let ctx = ContextInput {
        valid_from: "2026-09-10T00:00:00Z".to_string(),
        valid_until: "2026-09-09T00:00:00Z".to_string(),
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));
}

#[test]
fn test_phase_2_refusal_input_shape() {
    let pkg_bytes = read_fixture("refusal_p2_input_shape.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::InputShape));
}

#[test]
fn test_phase_3_refusal_unknown_format_never_unsupported_input() {
    let pkg_bytes = read_fixture("refusal_p3_unknown_format.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    // MUST refuse as unknown_format and NEVER degrade to unsupported_input
    assert_eq!(report.reason, Some(IncidentReason::UnknownFormat));
}

#[test]
fn test_phase_4_refusal_digest() {
    let pkg_bytes = read_fixture("refusal_p4_digest.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Digest));
}

#[test]
fn test_phase_5_refusal_locator() {
    let pkg_bytes = read_fixture("refusal_p5_locator.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Locator));
}

#[test]
fn test_phase_6_refusal_trust_input() {
    let pkg_bytes = read_fixture("refusal_p6_trust_input.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));
    assert_eq!(report.attestations.len(), 1);
    assert_eq!(report.attestations[0]["status"], "refused");
    assert_eq!(report.attestations[0]["input_id"], "my-attestation");
}

#[test]
fn test_phase_7_refusal_disclosure() {
    let pkg_bytes = read_fixture("refusal_p7_disclosure.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Disclosure));
}

#[test]
fn test_phase_8_refusal_cap1_rules() {
    let pkg_bytes = read_fixture("refusal_p8_cap1_rules.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Cap1Rules));
}

#[test]
fn test_phase_9_refusal_claim_boundary() {
    let pkg_bytes = read_fixture("refusal_p9_claim_boundary.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn test_phase_10_refusal_expectation_mismatch() {
    let pkg_bytes = read_fixture("valid_present_empty.tar");
    let ctx = ContextInput {
        expectations: vec![ContextExpectation {
            input_id: "empty-history".to_string(),
            sha256: "0".repeat(64), // Mismatched sha256
            require_disclosure: false,
        }],
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::ExpectationMismatch));
    assert_eq!(report.expectation, IncidentExpectation::Mismatched);
}

#[test]
fn test_phase_11_refusal_stale_assessment() {
    let pkg_bytes = read_fixture("refusal_p11_stale_assessment.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_ordering_p3_unknown_format_before_p4_digest() {
    let pkg_bytes = read_fixture("ordering_p3_unknown_format_and_p4_digest.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    // Package has BOTH Phase 3 fault (unknown format) AND Phase 4 fault (digest mismatch).
    // Section 10 order dictates Phase 3 wins.
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::UnknownFormat));
}

#[test]
fn test_ordering_p4_digest_before_p11_stale_assessment() {
    let pkg_bytes = read_fixture("ordering_p4_digest_and_p11_stale.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    // Package has BOTH Phase 4 fault (digest mismatch) AND Phase 11 fault (stale context sha).
    // Section 10 order dictates Phase 4 wins.
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Digest));
}

#[test]
fn test_ordering_p8_cap1_rules_before_p11_stale_assessment() {
    let pkg_bytes = read_fixture("ordering_p8_cap1_rules_and_p11_stale.tar");
    let ctx = ContextInput::default();
    let report = verify_incident_package(&pkg_bytes, &ctx);

    // Package has BOTH Phase 8 fault (duplicate stratum in cap1) AND Phase 11 fault (stale context sha).
    // Section 10 order dictates Phase 8 wins.
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::Cap1Rules));
}
