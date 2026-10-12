use assay_evidence::attestation::{sign_statement, statement_for_bundle};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::incident_package::{
    verify_incident_package, ContextApplicability, ContextExpectation, ContextInput, ContextLimits,
    ContextWindow, IncidentExpectation, IncidentOutcome, IncidentReason, PackageRef,
    NON_CLAIMS_DEFAULT,
};
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use base64::Engine;
use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePublicKey};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

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

    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
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

fn make_test_header(name: &str, size: usize, typeflag: u8) -> [u8; 512] {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_octal = format!("{:011o}\0", size);
    header[124..136].copy_from_slice(size_octal.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());
    header
}

fn pad_to_512(data: &[u8]) -> Vec<u8> {
    let mut v = data.to_vec();
    let rem = v.len() % 512;
    if rem != 0 {
        v.resize(v.len() + (512 - rem), 0);
    }
    v
}

fn build_incident_package_tar(
    inventory_bytes: &[u8],
    assessment_bytes: &[u8],
    mut objects: Vec<(String, Vec<u8>)>,
) -> Vec<u8> {
    objects.sort_by(|a, b| a.0.cmp(&b.0));

    let mut archive = Vec::new();
    archive.extend_from_slice(&make_test_header(
        "inventory.json",
        inventory_bytes.len(),
        b'0',
    ));
    archive.extend_from_slice(&pad_to_512(inventory_bytes));

    archive.extend_from_slice(&make_test_header(
        "assessment.json",
        assessment_bytes.len(),
        b'0',
    ));
    archive.extend_from_slice(&pad_to_512(assessment_bytes));

    for (path, data) in &objects {
        archive.extend_from_slice(&make_test_header(path, data.len(), b'0'));
        archive.extend_from_slice(&pad_to_512(data));
    }

    archive.extend_from_slice(&[0u8; 1024]);
    archive
}

fn make_test_bundle(run_id: &str) -> Vec<u8> {
    let producer = ProducerMeta {
        name: "test-producer".into(),
        version: "1.0.0".into(),
        git: None,
    };
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer).with_producer(producer.clone());
    writer.add_event(
        EvidenceEvent::new(
            "assay.profile.finished",
            "urn:assay:test",
            run_id,
            0,
            serde_json::json!({
                "source": "test",
                "status": "success",
            }),
        )
        .with_producer(&producer),
    );
    writer.finish().expect("bundle finish");
    buffer
}

fn make_coding_agent_bundle(files_coverage: &str) -> Vec<u8> {
    let producer = ProducerMeta {
        name: "test-producer".into(),
        version: "1.0.0".into(),
        git: None,
    };
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer).with_producer(producer.clone());
    writer.add_event(
        EvidenceEvent::new(
            "assay.coding_agent.evidence_pack.v0",
            "urn:assay:test",
            "run-coding-agent",
            0,
            serde_json::json!({
                "declared_scope": {
                    "allowed_files": [],
                    "allowed_commands": [],
                    "network": "denied",
                    "allowed_mcp_tools": [],
                    "authorized": true
                },
                "observed_effects": {
                    "files_changed": [],
                    "commands_executed": [],
                    "network_attempts": [],
                    "mcp_tool_calls": [],
                    "test_observed": false
                },
                "coverage": {
                    "files": files_coverage,
                    "commands": "observed",
                    "network": "observed",
                    "mcp_tools": "observed",
                    "test": "observed"
                },
                "source_class": "boundary_observed",
                "non_claims": []
            }),
        )
        .with_producer(&producer),
    );
    writer.finish().expect("bundle finish");
    buffer
}

fn build_context_basis_package_with_window(
    target: Option<(&str, Vec<u8>, &str)>,
    record_basis: bool,
    surface: &str,
    applicability_state: (&str, &str, &str),
    recorded_observation: &str,
    window: ContextWindow,
    reference_target: bool,
) -> (Vec<u8>, ContextInput, PackageRef) {
    let bundle_bytes = make_test_bundle("run-context-basis");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let (target_id, target_sha, target_format, target_locator, target_bytes) = match target {
        Some((format, bytes, locator)) => (
            "context-target".to_string(),
            hex::encode(Sha256::digest(&bytes)),
            format.to_string(),
            locator.to_string(),
            Some(bytes),
        ),
        None => (
            "bundle-context".to_string(),
            bundle_sha.clone(),
            "assay-bundle-v1".to_string(),
            "event:run-context-basis:0".to_string(),
            None,
        ),
    };
    let event_ref = if reference_target {
        PackageRef {
            input_id: target_id.clone(),
            sha256: target_sha.clone(),
            locator: target_locator,
        }
    } else {
        PackageRef {
            input_id: "bundle-context".to_string(),
            sha256: bundle_sha.clone(),
            locator: "event:run-context-basis:0".to_string(),
        }
    };
    let context = ContextInput {
        applicability: vec![ContextApplicability {
            surface: surface.to_string(),
            input_refs: vec![event_ref.clone()],
            window: Some(window.clone()),
            coverage: applicability_state.0.to_string(),
            retention: applicability_state.1.to_string(),
            correspondence: applicability_state.2.to_string(),
            source_class: "boundary_observed".to_string(),
        }],
        ..Default::default()
    };
    let mut context_bytes = assay_canonical::jcs::to_vec(&context).expect("canonical context");
    context_bytes.push(b'\n');
    let context_sha = hex::encode(Sha256::digest(&context_bytes));

    let recorded_basis = record_basis
        .then(|| event_ref.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let mut surfaces = [
        "tool_call",
        "filesystem",
        "network",
        "process",
        "transcript_join",
    ]
    .into_iter()
    .map(|name| {
        serde_json::json!({
            "basis_refs": [],
            "name": name,
            "observation": "unknown",
            "source_inputs": [],
            "window": null
        })
    })
    .collect::<Vec<_>>();
    let selected = surfaces
        .iter_mut()
        .find(|entry| entry["name"] == surface)
        .expect("known incident surface");
    selected["basis_refs"] = serde_json::to_value(recorded_basis).expect("basis refs");
    selected["observation"] = serde_json::json!(recorded_observation);
    selected["source_inputs"] = serde_json::json!(["bundle-context"]);
    selected["window"] = serde_json::to_value(&window).expect("window");
    let assessment = serde_json::json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha,
        "units": [],
        "surfaces": surfaces,
        "cap1": {
            "profile": "cap/1",
            "subject": {"kind": "collection", "ref": "incident-selected-inputs"},
            "strata": [{
                "id": surface,
                "population": "retained supplied records",
                "supports": ["positive_presence", "bounded_negative", "exhaustive_set"],
                "eligible": 0,
                "examined": 0,
                "unexamined": [],
                "basis": {"kind": "enumeration", "enumeration_method": "incident-v1-digest-locator-enumeration"}
            }],
            "integrity": {"complete": true, "statement": "accounting of selected supplied records", "capped_to": null}
        },
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment");
    assessment_bytes.push(b'\n');
    let assessment_sha = hex::encode(Sha256::digest(&assessment_bytes));
    let mut inputs = vec![serde_json::json!({
        "id": "bundle-context",
        "sha256": bundle_sha,
        "bytes": bundle_bytes.len(),
        "format": "assay-bundle-v1",
        "surfaces": [surface],
        "binding": null
    })];
    if let Some(bytes) = &target_bytes {
        inputs.push(serde_json::json!({
            "id": target_id,
            "sha256": target_sha,
            "bytes": bytes.len(),
            "format": target_format,
            "surfaces": [],
            "binding": null
        }));
    }
    inputs.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let inventory = serde_json::json!({
        "schema": "assay.incident.inventory.v1",
        "assessment_sha256": assessment_sha,
        "inputs": inputs,
        "context": {
            "actor": {"reason": "not_available", "refs": [], "value": null},
            "chain_gaps": {"reason": "not_available", "refs": [], "value": null},
            "clock": {"reason": "not_available", "refs": [], "value": null},
            "context_provenance": {"reason": "not_available", "refs": [], "value": null},
            "correlation": {"reason": "not_available", "refs": [], "value": null},
            "custody": {"reason": "not_available", "refs": [], "value": null},
            "denominator": {"reason": "not_available", "refs": [], "value": null},
            "missing_evidence": {"reason": "not_available", "refs": [], "value": null},
            "offline_verification": {"reason": "not_available", "refs": [], "value": null},
            "original_digests": {"reason": "not_available", "refs": [], "value": null},
            "policy_activation": {"reason": "not_available", "refs": [], "value": null},
            "producer_invocation": {"reason": "not_available", "refs": [], "value": null},
            "recoverability": {"reason": "not_available", "refs": [], "value": null},
            "resource_effects": {"reason": "not_available", "refs": [], "value": null},
            "retention": {"reason": "not_available", "refs": [], "value": null},
            "schema_versions": {"reason": "not_available", "refs": [], "value": null},
            "signature_basis": {"reason": "not_available", "refs": [], "value": null},
            "windows": {"reason": "not_available", "refs": [], "value": null},
            "withholding_derivatives": {"reason": "not_available", "refs": [], "value": null}
        },
        "non_claims": ["no_activity_completeness", "no_provider_outcome", "no_automatic_trust"]
    });
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory");
    inventory_bytes.push(b'\n');
    let mut objects = vec![(format!("objects/{bundle_sha}"), bundle_bytes)];
    if let Some(bytes) = target_bytes {
        objects.push((format!("objects/{target_sha}"), bytes));
    }
    let package = build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects);
    (package, context, event_ref)
}

fn build_context_basis_package(
    target: Option<(&str, Vec<u8>, &str)>,
    record_basis: bool,
    surface: &str,
    applicability_state: (&str, &str, &str),
    recorded_observation: &str,
) -> (Vec<u8>, ContextInput, PackageRef) {
    build_context_basis_package_with_window(
        target,
        record_basis,
        surface,
        applicability_state,
        recorded_observation,
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:01:00Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        true,
    )
}

#[test]
fn applicability_basis_omission_is_stale_assessment() {
    let (package, context, _) = build_context_basis_package(
        None,
        false,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn applicability_semantic_zero_window_remains_unknown_and_preserved() {
    let (package, context, _) = build_context_basis_package_with_window(
        None,
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "unknown",
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:00:00.000Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        true,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn applicability_accepts_typed_observation_health_root() {
    let health = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.runner.observation_health.v0",
        "run_id": "run-context-basis",
        "platform": "linux",
        "kernel_layer": "complete",
        "ringbuf_drops": 0,
        "policy_layer": "present",
        "sdk_layer": "present",
        "cgroup_correlation": "clean",
        "network_protocol_coverage": "connect_only",
        "network_endpoint_claim_scope": "peer_set",
        "notes": []
    }))
    .expect("health json");
    let (package, context, _) = build_context_basis_package(
        Some(("assay-health-json", health, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
}

#[test]
fn applicability_accepts_typed_enforcement_health_v0_root() {
    let health = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.enforcement_health.v0",
        "scope": "ipv4_tcp_connect",
        "network_enforcement": "active",
        "attach_confirmed": true,
        "blocked_count": 1,
        "allowed_count": 2,
        "enforcement_class": "strong"
    }))
    .expect("health json");
    let (package, context, _) = build_context_basis_package(
        Some(("assay-health-json", health, "json:")),
        true,
        "network",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
    assert_eq!(report.reason, None);
}

#[test]
fn applicability_accepts_typed_enforcement_health_v1_root() {
    let health = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.enforcement_health.v1",
        "status": "active",
        "mechanism": "landlock",
        "scope": "tcp_connect_landlock_port",
        "policy_semantics": "allowlist",
        "enforcement_class": "strong",
        "failure": null,
        "landlock": {
            "abi": 4,
            "handled_access_net": ["connect_tcp"],
            "allowed_connect_tcp_ports": [443],
            "no_new_privs_confirmed": true,
            "restrict_self_confirmed": true
        },
        "probe": null,
        "non_claims": []
    }))
    .expect("health json");
    let (package, context, _) = build_context_basis_package(
        Some(("assay-health-json", health, "json:")),
        true,
        "network",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
    assert_eq!(report.reason, None);
}

#[test]
fn applicability_accepts_matching_coverage_root() {
    let coverage = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.runner.coverage_descriptor.v0",
        "dimension": "filesystem",
        "method": "open tracepoint",
        "observes": ["path opens"],
        "known_blind_spots": [],
        "completeness": "full"
    }))
    .expect("coverage json");
    let (package, context, _) = build_context_basis_package(
        Some(("assay-coverage-json", coverage, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
}

#[test]
fn unreferenced_partial_coverage_limits_derived_observation() {
    let coverage = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.runner.coverage_descriptor.v0",
        "dimension": "filesystem",
        "method": "open tracepoint",
        "observes": ["path opens"],
        "known_blind_spots": ["mmap writes"],
        "completeness": "open_syscall_only"
    }))
    .expect("coverage json");
    let (package, context, _) = build_context_basis_package_with_window(
        Some(("assay-coverage-json", coverage, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:01:00Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        false,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn unreferenced_partial_coverage_allows_matching_limited_observation() {
    let coverage = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.runner.coverage_descriptor.v0",
        "dimension": "filesystem",
        "method": "open tracepoint",
        "observes": ["path opens"],
        "known_blind_spots": ["mmap writes"],
        "completeness": "open_syscall_only"
    }))
    .expect("coverage json");
    let (package, context, _) = build_context_basis_package_with_window(
        Some(("assay-coverage-json", coverage, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "limited",
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:01:00Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        false,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn unreferenced_partial_health_limits_derived_observation() {
    let health = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.runner.observation_health.v0",
        "run_id": "run-context-basis",
        "platform": "linux",
        "kernel_layer": "partial_ringbuf_drops",
        "ringbuf_drops": 1,
        "policy_layer": "present",
        "sdk_layer": "present",
        "cgroup_correlation": "clean",
        "network_protocol_coverage": "connect_only",
        "network_endpoint_claim_scope": "peer_set",
        "notes": []
    }))
    .expect("health json");
    let (package, context, _) = build_context_basis_package_with_window(
        Some(("assay-health-json", health, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:01:00Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        false,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn unreferenced_coding_agent_gap_limits_derived_observation() {
    let bundle = make_coding_agent_bundle("partial");
    let (package, context, _) = build_context_basis_package_with_window(
        Some(("assay-bundle-v1", bundle, "event:run-coding-agent:0")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
        ContextWindow {
            start: "2026-09-08T00:00:00Z".to_string(),
            end: "2026-09-08T00:01:00Z".to_string(),
            clock_basis: "retained profile boundary".to_string(),
        },
        false,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn applicability_accepts_closed_incident_context_entry() {
    let declaration = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.incident.context.v1",
        "entries": [{"kind": "retention", "value": "bounded", "source_refs": []}]
    }))
    .expect("context declaration");
    let (package, context, _) = build_context_basis_package(
        Some(("incident-context-v1", declaration, "json:/entries/0")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
}

fn build_test_package_with_attestation_ext(
    envelope_bytes: &[u8],
    bundle_bytes: &[u8],
    pem_bytes: &[u8],
    report: Option<(&[u8], Option<&str>, &str, &str)>,
) -> (Vec<u8>, ContextInput) {
    let bundle_sha = hex::encode(Sha256::digest(bundle_bytes));
    let pem_sha = hex::encode(Sha256::digest(pem_bytes));
    let envelope_sha = hex::encode(Sha256::digest(envelope_bytes));
    let report_sha = report.map(|r| hex::encode(Sha256::digest(r.0)));

    let context = ContextInput {
        keys: vec![pem_sha.clone()],
        ..Default::default()
    };
    let mut ctx_canonical = assay_canonical::jcs::to_vec(&context).expect("canonical context");
    ctx_canonical.push(b'\n');
    let ctx_sha = hex::encode(Sha256::digest(&ctx_canonical));

    let assessment_json = serde_json::json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": ctx_sha,
        "units": [],
        "surfaces": [
            {"basis_refs": [], "name": "tool_call", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "filesystem", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "network", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "process", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "transcript_join", "observation": "unknown", "source_inputs": ["empty-history"], "window": null}
        ],
        "cap1": {
            "profile": "cap/1",
            "subject": {"kind": "collection", "ref": "incident-selected-inputs"},
            "strata": [{
                "id": "transcript_join",
                "population": "retained supplied records",
                "supports": ["positive_presence", "bounded_negative", "exhaustive_set"],
                "eligible": 0,
                "examined": 0,
                "unexamined": [],
                "basis": {"kind": "enumeration", "enumeration_method": "incident-v1-digest-locator-enumeration"}
            }],
            "integrity": {"complete": true, "statement": "accounting of selected supplied records", "capped_to": null}
        },
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let mut assessment_bytes =
        assay_canonical::jcs::to_vec(&assessment_json).expect("jcs assessment");
    assessment_bytes.push(b'\n');
    let assessment_sha = hex::encode(Sha256::digest(&assessment_bytes));

    let mut inventory_inputs = vec![
        serde_json::json!({
            "binding": {
                "attestation_input": null,
                "bundle_input": "bundle-1",
                "key_input": "key-1"
            },
            "bytes": envelope_bytes.len(),
            "format": "dsse-attestation",
            "id": "attestation-1",
            "sha256": envelope_sha,
            "surfaces": []
        }),
        serde_json::json!({
            "binding": null,
            "bytes": bundle_bytes.len(),
            "format": "assay-bundle-v1",
            "id": "bundle-1",
            "sha256": bundle_sha,
            "surfaces": []
        }),
        serde_json::json!({
            "binding": null,
            "bytes": 3,
            "format": "inspector-protocol-793d103",
            "id": "empty-history",
            "sha256": "37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570",
            "surfaces": ["transcript_join"]
        }),
        serde_json::json!({
            "binding": null,
            "bytes": pem_bytes.len(),
            "format": "ed25519-public-key-pem",
            "id": "key-1",
            "sha256": pem_sha,
            "surfaces": []
        }),
    ];

    if let (Some((rb, att_in, bun_in, key_in)), Some(rsha)) = (report, report_sha.as_deref()) {
        inventory_inputs.push(serde_json::json!({
            "binding": {
                "attestation_input": att_in,
                "bundle_input": bun_in,
                "key_input": key_in
            },
            "bytes": rb.len(),
            "format": "attestation-report",
            "id": "report-1",
            "sha256": rsha,
            "surfaces": []
        }));
    }
    inventory_inputs.sort_by(|a, b| {
        a.get("id")
            .and_then(|v| v.as_str())
            .cmp(&b.get("id").and_then(|v| v.as_str()))
    });

    let inventory_json = serde_json::json!({
        "schema": "assay.incident.inventory.v1",
        "assessment_sha256": assessment_sha,
        "inputs": inventory_inputs,
        "context": {
            "actor": {"reason": "not_available", "refs": [], "value": null},
            "chain_gaps": {"reason": "not_available", "refs": [], "value": null},
            "clock": {"reason": "not_available", "refs": [], "value": null},
            "context_provenance": {"reason": "not_available", "refs": [], "value": null},
            "correlation": {"reason": "not_available", "refs": [], "value": null},
            "custody": {"reason": "not_available", "refs": [], "value": null},
            "denominator": {"reason": "not_available", "refs": [], "value": null},
            "missing_evidence": {"reason": "not_available", "refs": [], "value": null},
            "offline_verification": {"reason": "not_available", "refs": [], "value": null},
            "original_digests": {"reason": "not_available", "refs": [], "value": null},
            "policy_activation": {"reason": "not_available", "refs": [], "value": null},
            "producer_invocation": {"reason": "not_available", "refs": [], "value": null},
            "recoverability": {"reason": "not_available", "refs": [], "value": null},
            "resource_effects": {"reason": "not_available", "refs": [], "value": null},
            "retention": {"reason": "not_available", "refs": [], "value": null},
            "schema_versions": {"reason": "not_available", "refs": [], "value": null},
            "signature_basis": {"reason": "not_available", "refs": [], "value": null},
            "windows": {"reason": "not_available", "refs": [], "value": null},
            "withholding_derivatives": {"reason": "not_available", "refs": [], "value": null}
        },
        "non_claims": [
            "no_activity_completeness",
            "no_provider_outcome",
            "no_automatic_trust"
        ]
    });
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory_json).expect("jcs inventory");
    inventory_bytes.push(b'\n');

    let mut objects = vec![
        (
            "objects/37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570".to_string(),
            b"[]\n".to_vec(),
        ),
        (format!("objects/{bundle_sha}"), bundle_bytes.to_vec()),
        (format!("objects/{pem_sha}"), pem_bytes.to_vec()),
        (format!("objects/{envelope_sha}"), envelope_bytes.to_vec()),
    ];

    if let (Some((rb, _, _, _)), Some(rsha)) = (report, report_sha.as_deref()) {
        objects.push((format!("objects/{rsha}"), rb.to_vec()));
    }

    (
        build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects),
        context,
    )
}

fn build_test_package_with_attestation(
    envelope_bytes: &[u8],
    bundle_bytes: &[u8],
    pem_bytes: &[u8],
    report_bytes: Option<&[u8]>,
) -> (Vec<u8>, ContextInput) {
    build_test_package_with_attestation_ext(
        envelope_bytes,
        bundle_bytes,
        pem_bytes,
        report_bytes.map(|b| (b, Some("attestation-1"), "bundle-1", "key-1")),
    )
}

#[test]
fn test_phase_6_valid_attestation_signature_verified() {
    let bundle_bytes = make_test_bundle("run-valid");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();
    let pem_sha = hex::encode(Sha256::digest(&pem_bytes));

    let (pkg_bytes, ctx) =
        build_test_package_with_attestation(&envelope_bytes, &bundle_bytes, &pem_bytes, None);
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
    assert_eq!(report.attestations.len(), 1);
    let att = &report.attestations[0];
    assert_eq!(att["input_id"], "attestation-1");
    assert_eq!(att["key_sha256"], pem_sha);
    assert_eq!(att["status"], "verified");
    assert_eq!(att["signature_verified"], true);
    assert_eq!(att["subject_matched"], true);
    assert_eq!(att["artifact_sha256"], bundle_sha);
    assert_eq!(att["predicate_type"], statement.predicate_type);
    assert_eq!(att["subject_name"], statement.subject[0].name);
    assert_eq!(att["extent_stated"], false);
    assert_eq!(att["extent"], serde_json::Value::Null);
}

#[test]
fn test_phase_6_refusal_flipped_signature_byte() {
    let bundle_bytes = make_test_bundle("run-flipped");
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let mut envelope = sign_statement(&statement, &signing_key).expect("envelope");

    let mut sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&envelope.signatures[0].sig)
        .expect("decode sig");
    sig_bytes[0] ^= 1;
    envelope.signatures[0].sig = base64::engine::general_purpose::STANDARD.encode(&sig_bytes);

    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();
    let pem_sha = hex::encode(Sha256::digest(&pem_bytes));

    let (pkg_bytes, ctx) =
        build_test_package_with_attestation(&envelope_bytes, &bundle_bytes, &pem_bytes, None);
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::AttestationVerification));
    assert_eq!(report.attestations.len(), 1);
    let att = &report.attestations[0];
    assert_eq!(att["input_id"], "attestation-1");
    assert_eq!(att["key_sha256"], pem_sha);
    assert_eq!(att["status"], "refused");
    assert_eq!(att["signature_verified"], false);
    assert_eq!(att["subject_matched"], false);
    assert_eq!(att["artifact_sha256"], serde_json::Value::Null);
    assert_eq!(att["extent_stated"], false);
    assert_eq!(att["extent"], serde_json::Value::Null);
}

#[test]
fn test_phase_6_refusal_subject_mismatch() {
    let bundle_bytes_offered = make_test_bundle("run-offered");
    let bundle_bytes_different = make_test_bundle("run-different");
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes_different).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");

    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();
    let pem_sha = hex::encode(Sha256::digest(&pem_bytes));

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes_offered,
        &pem_bytes,
        None,
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::AttestationVerification));
    assert_eq!(report.attestations.len(), 1);
    let att = &report.attestations[0];
    assert_eq!(att["input_id"], "attestation-1");
    assert_eq!(att["key_sha256"], pem_sha);
    assert_eq!(att["status"], "refused");
    assert_eq!(att["signature_verified"], false);
    assert_eq!(att["subject_matched"], false);
    assert_eq!(att["artifact_sha256"], serde_json::Value::Null);
    assert_eq!(att["extent_stated"], false);
    assert_eq!(att["extent"], serde_json::Value::Null);
}

#[test]
fn test_phase_1_refusal_leap_second_timestamp() {
    let pkg_bytes = read_fixture("valid_present_empty.tar");
    let ctx = ContextInput {
        as_of: "2026-09-08T00:00:60Z".to_string(),
        ..Default::default()
    };
    let report = verify_incident_package(&pkg_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::TrustInput));
}

#[test]
fn test_phase_2_refusal_closed_vocabulary_observation_bogus() {
    let ass_json = serde_json::json!({
        "cap1": {
            "integrity": {"capped_to": null, "complete": true, "statement": "accounting of selected supplied records"},
            "profile": "cap/1",
            "strata": [{
                "basis": {"enumeration_method": "incident-v1-digest-locator-enumeration", "kind": "enumeration"},
                "eligible": 0,
                "examined": 0,
                "id": "transcript_join",
                "population": "retained supplied records",
                "supports": ["positive_presence", "bounded_negative", "exhaustive_set"],
                "unexamined": []
            }],
            "subject": {"kind": "collection", "ref": "incident-selected-inputs"}
        },
        "claims": [],
        "joins": [],
        "resolved_results": [],
        "schema": "assay.incident.assessment.v1",
        "surfaces": [
            {"basis_refs": [], "name": "tool_call", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "filesystem", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "network", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "process", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "transcript_join", "observation": "bogus", "source_inputs": ["empty-history"], "window": null}
        ],
        "units": [],
        "verification_context_sha256": "919787b66b4dc0688944d7c76218c6da168890f8251985ccfb12e2a92cfcb814"
    });
    let mut ass_bytes = assay_canonical::jcs::to_vec(&ass_json).unwrap();
    ass_bytes.push(b'\n');
    let ass_sha = hex::encode(Sha256::digest(&ass_bytes));

    let inv_json = serde_json::json!({
        "assessment_sha256": ass_sha,
        "context": {
            "actor": {"reason": "not_available", "refs": [], "value": null},
            "chain_gaps": {"reason": "not_available", "refs": [], "value": null},
            "clock": {"reason": "not_available", "refs": [], "value": null},
            "context_provenance": {"reason": "not_available", "refs": [], "value": null},
            "correlation": {"reason": "not_available", "refs": [], "value": null},
            "custody": {"reason": "not_available", "refs": [], "value": null},
            "denominator": {"reason": "not_available", "refs": [], "value": null},
            "missing_evidence": {"reason": "not_available", "refs": [], "value": null},
            "offline_verification": {"reason": "not_available", "refs": [], "value": null},
            "original_digests": {"reason": "not_available", "refs": [], "value": null},
            "policy_activation": {"reason": "not_available", "refs": [], "value": null},
            "producer_invocation": {"reason": "not_available", "refs": [], "value": null},
            "recoverability": {"reason": "not_available", "refs": [], "value": null},
            "resource_effects": {"reason": "not_available", "refs": [], "value": null},
            "retention": {"reason": "not_available", "refs": [], "value": null},
            "schema_versions": {"reason": "not_available", "refs": [], "value": null},
            "signature_basis": {"reason": "not_available", "refs": [], "value": null},
            "windows": {"reason": "not_available", "refs": [], "value": null},
            "withholding_derivatives": {"reason": "not_available", "refs": [], "value": null}
        },
        "inputs": [{
            "binding": null,
            "bytes": 3,
            "format": "inspector-protocol-793d103",
            "id": "empty-history",
            "sha256": "37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570",
            "surfaces": ["transcript_join"]
        }],
        "non_claims": ["no_activity_completeness", "no_provider_outcome", "no_automatic_trust"],
        "schema": "assay.incident.inventory.v1"
    });
    let mut inv_bytes = assay_canonical::jcs::to_vec(&inv_json).unwrap();
    inv_bytes.push(b'\n');

    let obj_data = b"[]\n";
    let obj_digest = "37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570";
    let objects = vec![(format!("objects/{obj_digest}"), obj_data.to_vec())];
    let archive = build_incident_package_tar(&inv_bytes, &ass_bytes, objects);

    let ctx = ContextInput::default();
    let report = verify_incident_package(&archive, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::InputShape));
}

#[test]
fn probe_forged_attestation_report_rides_verified_verdict() {
    test_incident_package_phase_11_refusal_forged_attestation_report();
}

#[test]
fn test_incident_package_phase_11_refusal_forged_attestation_report() {
    let bundle_bytes = make_test_bundle("run-probe-report");
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();
    // Forged retained report: claims verified/OK for the same bundle, no signature.
    let forged = b"{\"status\":\"verified\",\"signature_verified\":true,\"subject_matched\":true}";
    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&forged[..]),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);
    let reported_ids: Vec<&str> = report
        .attestations
        .iter()
        .filter_map(|r| r.get("input_id").and_then(|v| v.as_str()))
        .collect();
    assert!(
        !reported_ids.contains(&"report-1"),
        "forged report-1 got a row: {reported_ids:?}"
    );
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageRefused,
        "forged attestation-report refused the package"
    );
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_matching_attestation_report_verified() {
    let bundle_bytes = make_test_bundle("run-matching-report");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();
    let pem_sha = hex::encode(Sha256::digest(&pem_bytes));

    let matching_report = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&matching_report).expect("report json");

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
    assert_eq!(report.attestations.len(), 1);
    let att = &report.attestations[0];
    assert_eq!(att["input_id"], "attestation-1");
    assert_eq!(att["key_sha256"], pem_sha);
    assert_eq!(att["status"], "verified");
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_missing_binding() {
    let bundle_bytes = make_test_bundle("run-missing-binding");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let matching_report = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&matching_report).expect("report json");

    // Report names an unknown/missing attestation_input
    let (pkg_bytes, ctx) = build_test_package_with_attestation_ext(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some((
            &report_bytes,
            Some("attestation-nonexistent"),
            "bundle-1",
            "key-1",
        )),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_other_binding() {
    let bundle_bytes = make_test_bundle("run-other-binding");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let matching_report = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&matching_report).expect("report json");

    // Report names attestation-1, but bundle_input does not match attestation-1's binding
    let (pkg_bytes, ctx) = build_test_package_with_attestation_ext(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some((
            &report_bytes,
            Some("attestation-1"),
            "bundle-other",
            "key-1",
        )),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_different_subject_name() {
    let bundle_bytes = make_test_bundle("run-diff-subject");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_val = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": "different-subject-name",
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&report_val).expect("report json");

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_different_predicate_type() {
    let bundle_bytes = make_test_bundle("run-diff-predicate");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_val = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": "https://different.predicate/type/v1",
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&report_val).expect("report json");

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_missing_schema() {
    let bundle_bytes = make_test_bundle("run-missing-schema");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_val = serde_json::json!({
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
    });
    let report_bytes = serde_json::to_vec(&report_val).expect("report json");

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_unknown_claim_field() {
    let bundle_bytes = make_test_bundle("run-unknown-claim");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_val = serde_json::json!({
        "schema": "assay.evidence.attestation.verify.v1",
        "outcome": "attestation_verified",
        "signature_verified": true,
        "subject_matched": true,
        "artifact_sha256": bundle_sha,
        "predicate_type": statement.predicate_type,
        "subject_name": statement.subject[0].name,
        "extent_stated": false,
        "extent": null,
        "extra_claim": "unestablished_claim",
    });
    let report_bytes = serde_json::to_vec(&report_val).expect("report json");

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_duplicate_subject_name_forged_then_real(
) {
    let bundle_bytes = make_test_bundle("run-dup-subject");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_str = format!(
        r#"{{"schema":"assay.evidence.attestation.verify.v1","outcome":"attestation_verified","signature_verified":true,"subject_matched":true,"artifact_sha256":"{bundle_sha}","predicate_type":"{}","subject_name":"forged-bundle-id","subject_name":"{}","extent_stated":false,"extent":null}}"#,
        statement.predicate_type, statement.subject[0].name,
    );
    let report_bytes = report_str.into_bytes();

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_duplicate_key_unicode_escaped() {
    let bundle_bytes = make_test_bundle("run-dup-unicode");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_str = format!(
        r#"{{"schema":"assay.evidence.attestation.verify.v1","outcome":"attestation_verified","signature_verified":true,"subject_matched":true,"artifact_sha256":"{bundle_sha}","predicate_type":"{}","\u0073ubject_name":"forged-bundle-id","subject_name":"{}","extent_stated":false,"extent":null}}"#,
        statement.predicate_type, statement.subject[0].name,
    );
    let report_bytes = report_str.into_bytes();

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

#[test]
fn test_incident_package_phase_11_refusal_attestation_report_duplicate_signature_verified_string_then_bool(
) {
    let bundle_bytes = make_test_bundle("run-dup-sig");
    let bundle_sha = hex::encode(Sha256::digest(&bundle_bytes));
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle_bytes).expect("statement");
    let envelope = sign_statement(&statement, &signing_key).expect("envelope");
    let envelope_bytes = serde_json::to_vec(&envelope).expect("envelope json");
    let pem_str = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let pem_bytes = pem_str.into_bytes();

    let report_str = format!(
        r#"{{"schema":"assay.evidence.attestation.verify.v1","outcome":"attestation_verified","signature_verified":"true","signature_verified":true,"subject_matched":true,"artifact_sha256":"{bundle_sha}","predicate_type":"{}","subject_name":"{}","extent_stated":false,"extent":null}}"#,
        statement.predicate_type, statement.subject[0].name,
    );
    let report_bytes = report_str.into_bytes();

    let (pkg_bytes, ctx) = build_test_package_with_attestation(
        &envelope_bytes,
        &bundle_bytes,
        &pem_bytes,
        Some(&report_bytes),
    );
    let report = verify_incident_package(&pkg_bytes, &ctx);

    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::StaleAssessment));
}

// Admission tests use invalid outer bytes as the competing failure. Reaching
// input_shape proves only that context admission passed, not a valid assessment.
fn applicability_context() -> ContextInput {
    use assay_evidence::incident_package::{ContextApplicability, ContextWindow, PackageRef};
    ContextInput {
        applicability: vec![ContextApplicability {
            surface: "tool_call".into(),
            input_refs: vec![PackageRef {
                input_id: "context".into(),
                sha256: "a".repeat(64),
                locator: "json:".into(),
            }],
            window: Some(ContextWindow {
                start: "2026-10-12T00:00:00Z".into(),
                end: "2026-10-12T01:00:00Z".into(),
                clock_basis: "UTC".into(),
            }),
            coverage: "adequate".into(),
            retention: "adequate".into(),
            correspondence: "unique".into(),
            source_class: "boundary_observed".into(),
        }],
        ..ContextInput::default()
    }
}

#[test]
fn applicability_shape_refuses_before_outer_archive() {
    // Removing phase-1 applicability validation must expose input_shape instead.
    for (pointer, value) in [
        ("/window", serde_json::Value::Null),
        ("/window/clock_basis", serde_json::json!(" ")),
        ("/window/start", serde_json::json!("2026-10-13T00:00:00Z")),
        ("/coverage", serde_json::json!("complete")),
        ("/retention", serde_json::json!("complete")),
        ("/correspondence", serde_json::json!("yes")),
        ("/source_class", serde_json::json!("trusted")),
        ("/input_refs/0/input_id", serde_json::json!("../context")),
        ("/input_refs/0/sha256", serde_json::json!("A".repeat(64))),
        ("/input_refs/0/locator", serde_json::json!("\t")),
    ] {
        let mut ctx = applicability_context();
        let mut row = serde_json::to_value(&ctx.applicability[0]).unwrap();
        *row.pointer_mut(pointer).unwrap() = value;
        ctx.applicability[0] = serde_json::from_value(row).unwrap();
        let result = verify_incident_package(b"invalid outer archive", &ctx);
        assert_eq!(result.reason, Some(IncidentReason::TrustInput), "{pointer}");
        assert_eq!(result.outcome, IncidentOutcome::PackageRefused);
        assert!(result.counts.is_none());
        assert!(result.verification_context.is_none());
    }
}

#[test]
fn applicability_valid_vocabulary_reaches_outer_archive() {
    for (field, values) in [
        ("coverage", &["adequate", "limited", "unknown"][..]),
        ("retention", &["adequate", "limited", "unknown"][..]),
        ("correspondence", &["unique", "ambiguous", "unknown"][..]),
        (
            "source_class",
            &[
                "boundary_observed",
                "independently_observed",
                "third_party_observed",
                "producer_reported",
                "issuer_attested",
                "receiver_receipt",
                "unknown",
            ][..],
        ),
    ] {
        for value in values {
            let mut ctx = applicability_context();
            let mut row = serde_json::to_value(&ctx.applicability[0]).unwrap();
            row[field] = serde_json::json!(value);
            ctx.applicability[0] = serde_json::from_value(row).unwrap();
            assert_eq!(
                verify_incident_package(b"invalid outer archive", &ctx).reason,
                Some(IncidentReason::InputShape),
                "{field}={value}"
            );
        }
    }
    let mut ctx = applicability_context();
    let window = ctx.applicability[0].window.as_mut().unwrap();
    window.end = window.start.clone();
    assert_eq!(
        verify_incident_package(b"invalid outer archive", &ctx).reason,
        Some(IncidentReason::InputShape)
    );
    ctx.applicability.clear();
    assert_eq!(
        verify_incident_package(b"invalid outer archive", &ctx).reason,
        Some(IncidentReason::InputShape)
    );
}

#[test]
fn applicability_text_uses_utf8_bytes_and_hard_first_limits() {
    for pointer in ["/window/clock_basis", "/input_refs/0/locator"] {
        for (selected, text, expected) in [
            (8, "é".repeat(4), IncidentReason::InputShape),
            (
                8,
                format!("{}x", "é".repeat(4)),
                IncidentReason::ResourceSelected,
            ),
            (65536, "x".repeat(65536), IncidentReason::InputShape),
            (65536, "x".repeat(65537), IncidentReason::ResourceHard),
            (8, "x".repeat(65537), IncidentReason::ResourceHard),
        ] {
            let mut ctx = applicability_context();
            ctx.limits.non_payload_string_bytes = selected;
            let mut row = serde_json::to_value(&ctx.applicability[0]).unwrap();
            *row.pointer_mut(pointer).unwrap() = serde_json::json!(text);
            ctx.applicability[0] = serde_json::from_value(row).unwrap();
            assert_eq!(
                verify_incident_package(b"invalid outer archive", &ctx).reason,
                Some(expected),
                "{pointer}, selected={selected}"
            );
        }
    }
}
