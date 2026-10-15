use assay_evidence::attestation::{sign_statement, statement_for_bundle};
use assay_evidence::bundle::{BundleReader, BundleWriter};
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

fn rebind_package_context(package: &[u8], context: &ContextInput) -> Vec<u8> {
    let members = read_test_tar_members(package);
    let mut inventory: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "inventory.json")
            .expect("inventory")
            .1,
    )
    .expect("inventory JSON");
    let mut assessment: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "assessment.json")
            .expect("assessment")
            .1,
    )
    .expect("assessment JSON");
    let mut context_bytes = assay_canonical::jcs::to_vec(context).expect("context JCS");
    context_bytes.push(b'\n');
    assessment["verification_context_sha256"] =
        serde_json::json!(hex::encode(Sha256::digest(context_bytes)));
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment JCS");
    assessment_bytes.push(b'\n');
    inventory["assessment_sha256"] =
        serde_json::json!(hex::encode(Sha256::digest(&assessment_bytes)));
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    let objects = members
        .into_iter()
        .filter(|(name, _)| name.starts_with("objects/"))
        .collect();
    build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects)
}

fn replace_input_object(
    package: &[u8],
    input_id: &str,
    replacement: Vec<u8>,
    mutate_assessment: impl FnOnce(&mut serde_json::Value, &str),
) -> Vec<u8> {
    fn replace_string(value: &mut serde_json::Value, old: &str, new: &str) {
        match value {
            serde_json::Value::String(value) if value == old => *value = new.to_string(),
            serde_json::Value::Array(values) => {
                for value in values {
                    replace_string(value, old, new);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    replace_string(value, old, new);
                }
            }
            _ => {}
        }
    }

    let members = read_test_tar_members(package);
    let mut inventory: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "inventory.json")
            .expect("inventory")
            .1,
    )
    .expect("inventory JSON");
    let mut assessment: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "assessment.json")
            .expect("assessment")
            .1,
    )
    .expect("assessment JSON");
    let input = inventory["inputs"]
        .as_array_mut()
        .expect("inputs")
        .iter_mut()
        .find(|input| input["id"] == input_id)
        .expect("selected input");
    let old_sha = input["sha256"].as_str().expect("input sha").to_string();
    let new_sha = hex::encode(Sha256::digest(&replacement));
    input["sha256"] = serde_json::json!(new_sha);
    input["bytes"] = serde_json::json!(replacement.len());
    replace_string(&mut assessment, &old_sha, &new_sha);
    mutate_assessment(&mut assessment, &new_sha);
    for unit in assessment["units"].as_array_mut().expect("units") {
        let reference: PackageRef =
            serde_json::from_value(unit["source"].clone()).expect("unit source");
        unit["id"] = serde_json::json!(unit_id(&reference));
    }
    assessment["units"]
        .as_array_mut()
        .expect("units")
        .sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment JCS");
    assessment_bytes.push(b'\n');
    inventory["assessment_sha256"] =
        serde_json::json!(hex::encode(Sha256::digest(&assessment_bytes)));
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    let mut objects = members
        .into_iter()
        .filter(|(name, _)| name.starts_with("objects/") && name != &format!("objects/{old_sha}"))
        .collect::<Vec<_>>();
    objects.push((format!("objects/{new_sha}"), replacement));
    build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects)
}

fn input_object(package: &[u8], input_id: &str) -> Vec<u8> {
    let members = read_test_tar_members(package);
    let inventory: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "inventory.json")
            .expect("inventory")
            .1,
    )
    .expect("inventory JSON");
    let sha = inventory["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .find(|input| input["id"] == input_id)
        .expect("selected input")["sha256"]
        .as_str()
        .expect("input sha");
    members
        .into_iter()
        .find(|(name, _)| name == &format!("objects/{sha}"))
        .expect("input object")
        .1
}

fn read_test_tar_members(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut members = Vec::new();
    let mut offset = 0usize;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name_end = header[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let name = std::str::from_utf8(&header[..name_end])
            .expect("test tar path")
            .to_string();
        let size_text = std::str::from_utf8(&header[124..136])
            .expect("test tar size")
            .trim_matches(['\0', ' ']);
        let size = usize::from_str_radix(size_text, 8).expect("octal test tar size");
        let start = offset + 512;
        let end = start + size;
        members.push((name, bytes[start..end].to_vec()));
        offset = start + size.div_ceil(512) * 512;
    }
    members
}

#[derive(Clone, Copy)]
enum DisclosureControl {
    Reference,
    Withheld,
}

fn build_phase7_control(mode: DisclosureControl) -> (Vec<u8>, ContextInput) {
    let members = read_test_tar_members(&read_fixture("refusal_p7_disclosure.tar"));
    let mut inventory: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "inventory.json")
            .expect("inventory")
            .1,
    )
    .expect("inventory json");
    let mut assessment: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "assessment.json")
            .expect("assessment")
            .1,
    )
    .expect("assessment json");
    let results: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name.starts_with("objects/2e5e"))
            .expect("results")
            .1,
    )
    .expect("results json");
    let result = results["results"][0].clone();
    let target = serde_json::json!({
        "input_id": "disclosed-results",
        "locator": "json:/results/0",
        "sha256": "2e5e9cc3b58dd7a6dda803c2e89e9b7a05b361da5d0bf3dff3792c276044add7"
    });
    assessment["surfaces"][4]["source_inputs"] = serde_json::json!(["one-history"]);
    assessment["cap1"]["strata"][0]["eligible"] = serde_json::json!(1);
    match mode {
        DisclosureControl::Reference => {
            assessment["units"][0]["result"] =
                serde_json::json!({"mode": "reference", "target": target.clone()});
            assessment["cap1"]["strata"][0]["examined"] = serde_json::json!(1);
            assessment["resolved_results"] = serde_json::json!([target]);
        }
        DisclosureControl::Withheld => {
            let digest = hex::encode(Sha256::digest(
                assay_canonical::jcs::to_vec(&result).expect("fresh result JCS"),
            ));
            assessment["units"][0]["disposition"] = serde_json::json!("withheld");
            assessment["units"][0]["result"] = serde_json::Value::Null;
            assessment["units"][0]["withheld_digest"] = serde_json::json!(digest);
            assessment["cap1"]["strata"][0]["examined"] = serde_json::json!(0);
            assessment["cap1"]["strata"][0]["unexamined"] = serde_json::json!([{
                "disposition": "withheld",
                "unit": assessment["units"][0]["id"],
                "withheld_digest": digest
            }]);
            assessment["resolved_results"] = serde_json::json!([]);
        }
    }
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment JCS");
    assessment_bytes.push(b'\n');
    inventory["assessment_sha256"] =
        serde_json::json!(hex::encode(Sha256::digest(&assessment_bytes)));
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    let objects = members
        .into_iter()
        .filter(|(name, _)| name.starts_with("objects/"))
        .collect();
    (
        build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects),
        ContextInput::default(),
    )
}

#[test]
fn fresh_reference_and_withheld_disclosure_controls_verify() {
    for mode in [DisclosureControl::Reference, DisclosureControl::Withheld] {
        let (package, context) = build_phase7_control(mode);
        let report = verify_incident_package(&package, &context);
        assert_eq!(
            report.outcome,
            IncidentOutcome::PackageVerified,
            "{report:?}"
        );
    }
}

#[test]
fn referenced_result_sources_charge_the_shared_reference_budget() {
    let (base_package, mut context) = build_phase7_control(DisclosureControl::Reference);
    context.limits.references = 4;
    let package = rebind_package_context(&base_package, &context);
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );

    context.limits.references = 3;
    let package = rebind_package_context(&base_package, &context);
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ResourceSelected));
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

fn build_canonical_payload_package(
    payload: serde_json::Value,
    disposition: &str,
) -> (Vec<u8>, ContextInput) {
    build_canonical_payload_package_with_locator(
        payload,
        disposition,
        "run-canonical-payload",
        "event:run-canonical-payload:0",
    )
}

fn build_canonical_payload_package_with_locator(
    payload: serde_json::Value,
    disposition: &str,
    run_id: &str,
    locator: &str,
) -> (Vec<u8>, ContextInput) {
    let producer = ProducerMeta {
        name: "test-producer".into(),
        version: "1.0.0".into(),
        git: None,
    };
    let mut event = EvidenceEvent::new("assay.sandbox.exec", "urn:assay:test", run_id, 0, payload)
        .with_producer(&producer);
    event.subject = Some("sh".into());
    event.time = chrono::DateTime::parse_from_rfc3339("2026-10-12T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut bundle = Vec::new();
    let mut writer = BundleWriter::new(&mut bundle).with_producer(producer);
    writer.add_event(event);
    writer.finish().expect("canonical payload bundle");
    let reader = BundleReader::open(std::io::Cursor::new(bundle.clone())).expect("bundle reader");
    let original_event = serde_json::to_value(
        reader
            .events_vec()
            .expect("bundle events")
            .into_iter()
            .next()
            .expect("one event"),
    )
    .expect("event value");
    let events_sha256 = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(reader.events_raw()))
    );
    let bundle_sha = hex::encode(Sha256::digest(&bundle));
    let source = PackageRef {
        input_id: "bundle".into(),
        sha256: bundle_sha.clone(),
        locator: locator.into(),
    };
    let result = serde_json::json!({
        "operation": "assay-bundle-v1",
        "source": source,
        "value": {"event": original_event, "events_sha256": events_sha256}
    });
    let examined = disposition == "examined";
    let unit = serde_json::json!({
        "disposition": disposition,
        "id": unit_id(&source),
        "result": if examined {serde_json::json!({"mode": "inline", "value": result})} else {serde_json::Value::Null},
        "source": source,
        "surface": "process",
        "withheld_digest": null
    });
    let unexamined = if examined {
        serde_json::json!([])
    } else {
        serde_json::json!([{"disposition": disposition, "unit": unit["id"]}])
    };
    let cap1 = serde_json::json!({
        "integrity": {
            "capped_to": if examined {serde_json::Value::Null} else {serde_json::json!("incomplete")},
            "complete": examined,
            "statement": "accounting of selected supplied records"
        },
        "profile": "cap/1",
        "strata": [{
            "basis": {"enumeration_method": "incident-v1-digest-locator-enumeration", "kind": "enumeration"},
            "eligible": 1,
            "examined": if examined {1} else {0},
            "id": "process",
            "population": "retained supplied records",
            "supports": ["positive_presence", "bounded_negative", "exhaustive_set"],
            "unexamined": unexamined
        }],
        "subject": {"kind": "collection", "ref": "incident-selected-inputs"}
    });
    let context = ContextInput::default();
    let mut context_bytes = assay_canonical::jcs::to_vec(&context).expect("context JCS");
    context_bytes.push(b'\n');
    let assessment = serde_json::json!({
        "cap1": cap1,
        "claims": [],
        "joins": [],
        "resolved_results": [],
        "schema": "assay.incident.assessment.v1",
        "surfaces": [
            {"basis_refs": [], "name": "tool_call", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "filesystem", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "network", "observation": "unknown", "source_inputs": [], "window": null},
            {"basis_refs": [], "name": "process", "observation": "unknown", "source_inputs": ["bundle"], "window": null},
            {"basis_refs": [], "name": "transcript_join", "observation": "unknown", "source_inputs": [], "window": null}
        ],
        "units": [unit],
        "verification_context_sha256": hex::encode(Sha256::digest(context_bytes))
    });
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment JCS");
    assessment_bytes.push(b'\n');
    let inventory = serde_json::json!({
        "assessment_sha256": hex::encode(Sha256::digest(&assessment_bytes)),
        "context": empty_incident_context(),
        "inputs": [{"binding": null, "bytes": bundle.len(), "format": "assay-bundle-v1", "id": "bundle", "sha256": bundle_sha, "surfaces": ["process"]}],
        "non_claims": NON_CLAIMS_DEFAULT,
        "schema": "assay.incident.inventory.v1"
    });
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    (
        build_incident_package_tar(
            &inventory_bytes,
            &assessment_bytes,
            vec![(format!("objects/{bundle_sha}"), bundle)],
        ),
        context,
    )
}

#[test]
fn malformed_canonical_payload_is_a_failed_unit_not_an_examined_result() {
    let malformed = serde_json::json!({"argv0": "sh"});
    let (failed_package, context) = build_canonical_payload_package(malformed.clone(), "failed");
    assert_eq!(
        verify_incident_package(&failed_package, &context).outcome,
        IncidentOutcome::PackageVerified
    );

    let (examined_package, context) = build_canonical_payload_package(malformed, "examined");
    let report = verify_incident_package(&examined_package, &context);
    assert_eq!(
        report.reason,
        Some(IncidentReason::Disclosure),
        "{report:?}"
    );
}

#[test]
fn valid_canonical_payload_remains_examinable() {
    let (package, context) =
        build_canonical_payload_package(serde_json::json!({"argv0": "sh", "hits": 1}), "examined");
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn canonical_event_locator_percent_encodes_run_id() {
    let (package, context) = build_canonical_payload_package_with_locator(
        serde_json::json!({"argv0": "sh", "hits": 1}),
        "examined",
        "run/é space",
        "event:run%2F%C3%A9%20space:0",
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn canonical_event_locator_rejects_noncanonical_spelling() {
    for locator in ["event:run/é space:0", "event:run%2f%c3%a9%20space:0"] {
        let (package, context) = build_canonical_payload_package_with_locator(
            serde_json::json!({"argv0": "sh", "hits": 1}),
            "examined",
            "run/é space",
            locator,
        );
        let report = verify_incident_package(&package, &context);
        assert_eq!(
            report.reason,
            Some(IncidentReason::Locator),
            "{locator}: {report:?}"
        );
    }
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

#[derive(Clone, Copy)]
enum ClaimScenario {
    Positive,
    BoundedNegative,
    Exhaustive,
}

fn make_join_context_bundle(degraded: bool) -> Vec<u8> {
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
            "run-join-context",
            0,
            serde_json::json!({"source": "test", "status": "success"}),
        )
        .with_producer(&producer),
    );
    if degraded {
        writer.add_event(
            EvidenceEvent::new(
                "assay.sandbox.degraded",
                "urn:assay:test",
                "run-join-context",
                1,
                serde_json::json!({"reason": "weaker containment"}),
            )
            .with_producer(&producer),
        );
    }
    writer.finish().expect("join context bundle");
    buffer
}

fn unit_id(reference: &PackageRef) -> String {
    let bytes = assay_canonical::jcs::to_vec(&serde_json::json!([
        reference.input_id,
        reference.sha256,
        reference.locator
    ]))
    .expect("unit id input");
    hex::encode(Sha256::digest(bytes))
}

fn empty_incident_context() -> serde_json::Value {
    serde_json::json!({
        "actor":{"reason":"not_available","refs":[],"value":null},
        "chain_gaps":{"reason":"not_available","refs":[],"value":null},
        "clock":{"reason":"not_available","refs":[],"value":null},
        "context_provenance":{"reason":"not_available","refs":[],"value":null},
        "correlation":{"reason":"not_available","refs":[],"value":null},
        "custody":{"reason":"not_available","refs":[],"value":null},
        "denominator":{"reason":"not_available","refs":[],"value":null},
        "missing_evidence":{"reason":"not_available","refs":[],"value":null},
        "offline_verification":{"reason":"not_available","refs":[],"value":null},
        "original_digests":{"reason":"not_available","refs":[],"value":null},
        "policy_activation":{"reason":"not_available","refs":[],"value":null},
        "producer_invocation":{"reason":"not_available","refs":[],"value":null},
        "recoverability":{"reason":"not_available","refs":[],"value":null},
        "resource_effects":{"reason":"not_available","refs":[],"value":null},
        "retention":{"reason":"not_available","refs":[],"value":null},
        "schema_versions":{"reason":"not_available","refs":[],"value":null},
        "signature_basis":{"reason":"not_available","refs":[],"value":null},
        "windows":{"reason":"not_available","refs":[],"value":null},
        "withholding_derivatives":{"reason":"not_available","refs":[],"value":null}
    })
}

fn scalar_fixture_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => panic!("fixture scalar must be a string, number, or boolean"),
    }
}

fn build_join_claim_package_with_options(
    scenario: ClaimScenario,
    partial_correlation: bool,
    enforcement_degraded: bool,
    mutate: impl FnOnce(&mut serde_json::Value),
) -> (Vec<u8>, ContextInput) {
    build_join_claim_package_with_source_options(
        scenario,
        partial_correlation,
        enforcement_degraded,
        serde_json::json!("call-1"),
        serde_json::json!("call-1"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        None,
        mutate,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_join_claim_package_with_source_options(
    scenario: ClaimScenario,
    partial_correlation: bool,
    enforcement_degraded: bool,
    tool_call_id: serde_json::Value,
    inspector_tool_call_id: serde_json::Value,
    proxy_time: &str,
    inspector_time: &str,
    inspector_tool_name: &str,
    second_proxy_time: Option<&str>,
    mutate: impl FnOnce(&mut serde_json::Value),
) -> (Vec<u8>, ContextInput) {
    let context_bundle = make_join_context_bundle(enforcement_degraded);
    let context_sha = hex::encode(Sha256::digest(&context_bundle));
    let proxy_value = serde_json::json!({
        "data": {
            "decision": "allow",
            "reason_code": "allowed",
            "tool": "list_files",
            "tool_call_id": tool_call_id.clone()
        },
        "id": "decision-1",
        "source": "urn:assay:test",
        "specversion": "1.0",
        "time": proxy_time,
        "type": "assay.tool.decision"
    });
    let mut proxy_bytes = assay_canonical::jcs::to_vec(&proxy_value).expect("proxy JCS");
    let proxy_line_len = proxy_bytes.len();
    proxy_bytes.push(b'\n');
    let second_proxy_value = second_proxy_time.map(|time| {
        serde_json::json!({
            "data": {
                "decision": "allow",
                "reason_code": "allowed",
                "tool": "list_files",
                "tool_call_id": tool_call_id.clone()
            },
            "id": "decision-2",
            "source": "urn:assay:test",
            "specversion": "1.0",
            "time": time,
            "type": "assay.tool.decision"
        })
    });
    let second_proxy_line_len = if let Some(value) = &second_proxy_value {
        let encoded = assay_canonical::jcs::to_vec(value).expect("second proxy JCS");
        let len = encoded.len();
        proxy_bytes.extend_from_slice(&encoded);
        proxy_bytes.push(b'\n');
        Some(len)
    } else {
        None
    };
    let proxy_sha = hex::encode(Sha256::digest(&proxy_bytes));
    let inspector_tool_id = if matches!(scenario, ClaimScenario::BoundedNegative) {
        serde_json::json!("another-call")
    } else {
        inspector_tool_call_id
    };
    let inspector_value = serde_json::json!({
        "direction": "request",
        "id": "entry-1",
        "message": {
            "id": "rpc-1",
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "arguments": {"_meta": {"tool_call_id": inspector_tool_id}},
                "name": inspector_tool_name
            }
        },
        "origin": "client",
        "timestamp": inspector_time
    });
    let mut inspector_bytes =
        assay_canonical::jcs::to_vec(&serde_json::json!([inspector_value.clone()]))
            .expect("inspector JCS");
    inspector_bytes.push(b'\n');
    let inspector_sha = hex::encode(Sha256::digest(&inspector_bytes));

    let proxy_ref = PackageRef {
        input_id: "proxy".into(),
        sha256: proxy_sha.clone(),
        locator: format!("line:0:0:{proxy_line_len}"),
    };
    let second_proxy_ref = second_proxy_line_len.map(|line_len| PackageRef {
        input_id: "proxy".into(),
        sha256: proxy_sha.clone(),
        locator: format!("line:1:{}:{line_len}", proxy_line_len + 1),
    });
    let inspector_ref = PackageRef {
        input_id: "inspector".into(),
        sha256: inspector_sha.clone(),
        locator: "json:/0".into(),
    };
    let context_ref = PackageRef {
        input_id: "context-bundle".into(),
        sha256: context_sha.clone(),
        locator: "event:run-join-context:0".into(),
    };
    let window = ContextWindow {
        start: "2026-09-08T00:00:00Z".into(),
        end: "2026-09-08T00:00:03Z".into(),
        clock_basis: "fixture-clock".into(),
    };
    let context = ContextInput {
        applicability: vec![ContextApplicability {
            surface: "transcript_join".into(),
            input_refs: vec![context_ref.clone()],
            window: Some(window.clone()),
            coverage: "adequate".into(),
            retention: "adequate".into(),
            correspondence: "unique".into(),
            source_class: "boundary_observed".into(),
        }],
        ..Default::default()
    };
    let mut context_bytes = assay_canonical::jcs::to_vec(&context).expect("context JCS");
    context_bytes.push(b'\n');
    let context_digest = hex::encode(Sha256::digest(context_bytes));

    let mut units = vec![
        serde_json::json!({
            "disposition": "examined",
            "id": unit_id(&proxy_ref),
            "result": {"mode": "inline", "value": {
                "operation": "proxy-decision-ndjson",
                "source": proxy_ref,
                "value": proxy_value
            }},
            "source": proxy_ref,
            "surface": "tool_call",
            "withheld_digest": null
        }),
        serde_json::json!({
            "disposition": "examined",
            "id": unit_id(&inspector_ref),
            "result": {"mode": "inline", "value": {
                "operation": "inspector-protocol-793d103",
                "source": inspector_ref,
                "value": inspector_value
            }},
            "source": inspector_ref,
            "surface": "transcript_join",
            "withheld_digest": null
        }),
    ];
    if let (Some(reference), Some(value)) = (&second_proxy_ref, &second_proxy_value) {
        units.push(serde_json::json!({
            "disposition": "examined",
            "id": unit_id(reference),
            "result": {"mode": "inline", "value": {
                "operation": "proxy-decision-ndjson",
                "source": reference,
                "value": value
            }},
            "source": reference,
            "surface": "tool_call",
            "withheld_digest": null
        }));
    }
    units.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));

    let negative = matches!(scenario, ClaimScenario::BoundedNegative);
    let contract_refs = if negative {
        vec![format!("{}#{}", proxy_ref.input_id, proxy_ref.locator)]
    } else {
        vec![
            format!("{}#{}", proxy_ref.input_id, proxy_ref.locator),
            format!("{}#{}", inspector_ref.input_id, inspector_ref.locator),
        ]
    };
    let join = serde_json::json!({
        "contract": {
            "evidence_refs": contract_refs,
            "fallback_used": false,
            "join_grade": if negative {"failed"} else {"strong"},
            "join_key": "tool_call_id",
            "join_value": if negative {serde_json::Value::Null} else {serde_json::json!(scalar_fixture_value(&tool_call_id))},
            "left_artifact_role": "external_receipt",
            "notes": [],
            "right_artifact_role": "external_receipt",
            "schema": "assay.observability.join_result.v0",
            "scope": "tool_call",
            "unique_within_scope": true
        },
        "id": "join-1",
        "left": proxy_ref,
        "proposition": {
            "expected": tool_call_id,
            "kind": "counterpart_present",
            "left_pointer": "/data/tool_call_id",
            "right_pointer": "/message/params/arguments/_meta/tool_call_id",
            "surface": "transcript_join"
        },
        "result": if negative {"contradicted"} else {"confirmed"},
        "right": if negative {serde_json::Value::Null} else {serde_json::to_value(&inspector_ref).expect("ref")}
    });
    let (claim_kind, claim_decision) = match scenario {
        ClaimScenario::Positive => ("positive_presence", "supported"),
        ClaimScenario::BoundedNegative => ("bounded_negative", "supported"),
        ClaimScenario::Exhaustive => ("exhaustive_set", "supported"),
    };
    let mut cap1 = serde_json::json!({
        "integrity": {"capped_to": null, "complete": true, "statement": "accounting of selected supplied records"},
        "profile": "cap/1",
        "strata": [
            {"basis": {"enumeration_method": "incident-v1-digest-locator-enumeration", "kind": "enumeration"}, "eligible": if second_proxy_ref.is_some() {2} else {1}, "examined": if second_proxy_ref.is_some() {2} else {1}, "id": "tool_call", "population": "retained supplied records", "supports": ["positive_presence", "bounded_negative", "exhaustive_set"], "unexamined": []},
            {"basis": {"enumeration_method": "incident-v1-digest-locator-enumeration", "kind": "enumeration"}, "eligible": 1, "examined": 1, "id": "transcript_join", "population": "retained supplied records", "supports": ["positive_presence", "bounded_negative", "exhaustive_set"], "unexamined": []}
        ],
        "subject": {"kind": "collection", "ref": "incident-selected-inputs"}
    });
    if negative {
        cap1.as_object_mut().expect("CAP-1 object").insert(
            "absence_assertions".into(),
            serde_json::json!([{"assertion": "claim-1", "stratum": "transcript_join"}]),
        );
    }
    let surfaces = serde_json::json!([
        {"basis_refs": [], "name": "tool_call", "observation": "unknown", "source_inputs": ["proxy"], "window": null},
        {"basis_refs": [], "name": "filesystem", "observation": "unknown", "source_inputs": [], "window": null},
        {"basis_refs": [], "name": "network", "observation": "unknown", "source_inputs": [], "window": null},
        {"basis_refs": [], "name": "process", "observation": "unknown", "source_inputs": [], "window": null},
        {"basis_refs": [context_ref], "name": "transcript_join", "observation": "adequate", "source_inputs": ["inspector"], "window": window}
    ]);
    let mut assessment = serde_json::json!({
        "cap1": cap1,
        "claims": [{"decision": claim_decision, "evidence_refs": [proxy_ref], "id": "claim-1", "kind": claim_kind, "proposition": "join-1", "stratum": "transcript_join", "surface": "transcript_join"}],
        "joins": [join],
        "resolved_results": [],
        "schema": "assay.incident.assessment.v1",
        "surfaces": surfaces,
        "units": units,
        "verification_context_sha256": context_digest
    });
    mutate(&mut assessment);
    let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).expect("assessment JCS");
    assessment_bytes.push(b'\n');
    let assessment_sha = hex::encode(Sha256::digest(&assessment_bytes));
    let mut inputs = vec![
        serde_json::json!({"binding": null, "bytes": context_bundle.len(), "format": "assay-bundle-v1", "id": "context-bundle", "sha256": context_sha, "surfaces": []}),
        serde_json::json!({"binding": null, "bytes": inspector_bytes.len(), "format": "inspector-protocol-793d103", "id": "inspector", "sha256": inspector_sha, "surfaces": ["transcript_join"]}),
        serde_json::json!({"binding": null, "bytes": proxy_bytes.len(), "format": "proxy-decision-ndjson", "id": "proxy", "sha256": proxy_sha, "surfaces": ["tool_call"]}),
    ];
    let mut objects = vec![
        (format!("objects/{context_sha}"), context_bundle),
        (format!("objects/{inspector_sha}"), inspector_bytes),
        (format!("objects/{proxy_sha}"), proxy_bytes),
    ];
    if partial_correlation {
        let health = serde_json::to_vec(&serde_json::json!({
            "schema": "assay.runner.observation_health.v0",
            "run_id": "run-join-context",
            "platform": "linux",
            "kernel_layer": "complete",
            "ringbuf_drops": 0,
            "policy_layer": "present",
            "sdk_layer": "present",
            "cgroup_correlation": "partial",
            "network_protocol_coverage": "connect_only",
            "network_endpoint_claim_scope": "peer_set",
            "notes": []
        }))
        .expect("health JSON");
        let health_sha = hex::encode(Sha256::digest(&health));
        inputs.push(serde_json::json!({"binding": null, "bytes": health.len(), "format": "assay-health-json", "id": "health", "sha256": health_sha, "surfaces": []}));
        objects.push((format!("objects/{health_sha}"), health));
    }
    inputs.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let inventory = serde_json::json!({
        "assessment_sha256": assessment_sha,
        "context": {
            "actor":{"reason":"not_available","refs":[],"value":null},"chain_gaps":{"reason":"not_available","refs":[],"value":null},"clock":{"reason":"not_available","refs":[],"value":null},"context_provenance":{"reason":"not_available","refs":[],"value":null},"correlation":{"reason":"not_available","refs":[],"value":null},"custody":{"reason":"not_available","refs":[],"value":null},"denominator":{"reason":"not_available","refs":[],"value":null},"missing_evidence":{"reason":"not_available","refs":[],"value":null},"offline_verification":{"reason":"not_available","refs":[],"value":null},"original_digests":{"reason":"not_available","refs":[],"value":null},"policy_activation":{"reason":"not_available","refs":[],"value":null},"producer_invocation":{"reason":"not_available","refs":[],"value":null},"recoverability":{"reason":"not_available","refs":[],"value":null},"resource_effects":{"reason":"not_available","refs":[],"value":null},"retention":{"reason":"not_available","refs":[],"value":null},"schema_versions":{"reason":"not_available","refs":[],"value":null},"signature_basis":{"reason":"not_available","refs":[],"value":null},"windows":{"reason":"not_available","refs":[],"value":null},"withholding_derivatives":{"reason":"not_available","refs":[],"value":null}
        },
        "inputs": inputs,
        "non_claims": ["no_activity_completeness", "no_provider_outcome", "no_automatic_trust"],
        "schema": "assay.incident.inventory.v1"
    });
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    (
        build_incident_package_tar(&inventory_bytes, &assessment_bytes, objects),
        context,
    )
}

fn build_join_claim_package_with_mutation(
    scenario: ClaimScenario,
    mutate: impl FnOnce(&mut serde_json::Value),
) -> (Vec<u8>, ContextInput) {
    build_join_claim_package_with_options(scenario, false, false, mutate)
}

fn build_join_claim_package(scenario: ClaimScenario) -> (Vec<u8>, ContextInput) {
    build_join_claim_package_with_mutation(scenario, |_| {})
}

#[test]
fn derived_claim_controls_verify() {
    for scenario in [
        ClaimScenario::Positive,
        ClaimScenario::BoundedNegative,
        ClaimScenario::Exhaustive,
    ] {
        let (package, context) = build_join_claim_package(scenario);
        let report = verify_incident_package(&package, &context);
        assert_eq!(
            report.outcome,
            IncidentOutcome::PackageVerified,
            "{report:?}"
        );
    }
}

#[test]
fn mixed_source_records_share_one_selected_package_budget() {
    let (base_package, mut context) = build_join_claim_package(ClaimScenario::Positive);
    context.limits.records = 3;
    let package = rebind_package_context(&base_package, &context);
    let exact = verify_incident_package(&package, &context);
    assert_eq!(exact.outcome, IncidentOutcome::PackageVerified, "{exact:?}");
    context.limits.records = 2;
    let package = rebind_package_context(&base_package, &context);
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ResourceSelected));
}

#[test]
fn inspector_selected_preflight_precedes_locator_materialization() {
    let (base_package, mut context) = build_join_claim_package(ClaimScenario::Positive);
    let mut malformed_inspector = input_object(&base_package, "inspector");
    malformed_inspector.extend_from_slice(b"{");
    let package = replace_input_object(&base_package, "inspector", malformed_inspector, |_, _| {});
    context.limits.transcript_bytes = 1;
    let package = rebind_package_context(&package, &context);
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ResourceSelected));
}

#[test]
fn counterpart_expected_value_preserves_json_type() {
    let arguments = (
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("1"),
        serde_json::json!("1"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        None,
    );
    let (control, context) = build_join_claim_package_with_source_options(
        arguments.0,
        arguments.1,
        arguments.2,
        arguments.3.clone(),
        arguments.4.clone(),
        arguments.5,
        arguments.6,
        arguments.7,
        arguments.8,
        |_| {},
    );
    assert_eq!(
        verify_incident_package(&control, &context).outcome,
        IncidentOutcome::PackageVerified
    );
    let (mismatch, context) = build_join_claim_package_with_source_options(
        arguments.0,
        arguments.1,
        arguments.2,
        arguments.3,
        arguments.4,
        arguments.5,
        arguments.6,
        arguments.7,
        arguments.8,
        |assessment| assessment["joins"][0]["proposition"]["expected"] = serde_json::json!(1),
    );
    assert_eq!(
        verify_incident_package(&mismatch, &context).reason,
        Some(IncidentReason::ClaimBoundary)
    );
}

fn assert_matching_non_string_counterpart_value_remains_unobservable(value: serde_json::Value) {
    let (base_package, context) = build_join_claim_package(ClaimScenario::Positive);
    let mut inspector: serde_json::Value =
        serde_json::from_slice(&input_object(&base_package, "inspector")).expect("inspector JSON");
    let first = inspector[0].as_object_mut().expect("first Inspector entry");
    first["message"]["params"]["arguments"]["_meta"]["tool_call_id"] = value.clone();
    let mut second = serde_json::Value::Object(first.clone());
    second["id"] = serde_json::json!("entry-2");
    second["timestamp"] = serde_json::json!("2026-09-08T00:00:02.500Z");
    second["message"]["id"] = serde_json::json!("rpc-2");
    inspector
        .as_array_mut()
        .expect("Inspector array")
        .push(second);
    let mut inspector_bytes = assay_canonical::jcs::to_vec(&inspector).expect("Inspector JCS");
    inspector_bytes.push(b'\n');
    let package = replace_input_object(
        &base_package,
        "inspector",
        inspector_bytes,
        |assessment, inspector_sha| {
            let units = assessment["units"].as_array_mut().expect("units");
            let first_index = units
                .iter()
                .position(|unit| unit["source"]["input_id"] == "inspector")
                .expect("Inspector unit");
            units[first_index]["result"]["value"]["value"] = inspector[0].clone();
            let mut second_unit = units[first_index].clone();
            second_unit["source"]["locator"] = serde_json::json!("json:/1");
            second_unit["result"]["value"]["source"]["locator"] = serde_json::json!("json:/1");
            second_unit["result"]["value"]["value"] = inspector[1].clone();
            units.push(second_unit);

            let first_ref = serde_json::json!({
                "input_id": "inspector",
                "locator": "json:/0",
                "sha256": inspector_sha,
            });
            assessment["joins"][0] = serde_json::json!({
                "contract": {
                    "evidence_refs": [format!("inspector#json:/0")],
                    "fallback_used": false,
                    "join_grade": "failed",
                    "join_key": "tool_call_id",
                    "join_value": null,
                    "left_artifact_role": "external_receipt",
                    "notes": [],
                    "right_artifact_role": "external_receipt",
                    "schema": "assay.observability.join_result.v0",
                    "scope": "tool_call",
                    "unique_within_scope": false,
                },
                "id": "join-1",
                "left": first_ref,
                "proposition": {
                    "expected": value,
                    "kind": "counterpart_present",
                    "left_pointer": "/message/params/arguments/_meta/tool_call_id",
                    "right_pointer": "/message/params/arguments/_meta/tool_call_id",
                    "surface": "transcript_join",
                },
                "result": "unobservable",
                "right": null,
            });
            assessment["claims"] = serde_json::json!([]);
            let transcript = assessment["cap1"]["strata"]
                .as_array_mut()
                .expect("CAP-1 strata")
                .iter_mut()
                .find(|stratum| stratum["id"] == "transcript_join")
                .expect("transcript stratum");
            transcript["eligible"] = serde_json::json!(2);
            transcript["examined"] = serde_json::json!(2);
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "value={value:?}: {report:?}"
    );
}

#[test]
fn matching_numeric_counterpart_values_remain_unobservable() {
    assert_matching_non_string_counterpart_value_remains_unobservable(serde_json::json!(1));
}

#[test]
fn matching_boolean_counterpart_values_remain_unobservable() {
    assert_matching_non_string_counterpart_value_remains_unobservable(serde_json::json!(true));
}

#[test]
fn null_eligible_tool_identity_remains_weak_and_unobservable() {
    let (base_package, context) = build_join_claim_package(ClaimScenario::Positive);
    let mut inspector: serde_json::Value =
        serde_json::from_slice(&input_object(&base_package, "inspector")).expect("inspector JSON");
    inspector[0]["message"]["params"]["arguments"]["_meta"]["tool_call_id"] =
        serde_json::Value::Null;
    let mut inspector_bytes = assay_canonical::jcs::to_vec(&inspector).expect("Inspector JCS");
    inspector_bytes.push(b'\n');
    let package = replace_input_object(
        &base_package,
        "inspector",
        inspector_bytes,
        |assessment, _| {
            let inspector_unit = assessment["units"]
                .as_array_mut()
                .expect("units")
                .iter_mut()
                .find(|unit| unit["source"]["input_id"] == "inspector")
                .expect("Inspector unit");
            inspector_unit["result"]["value"]["value"] = inspector[0].clone();
            let inspector_ref = inspector_unit["source"].clone();
            assessment["joins"][0] = serde_json::json!({
                "contract": {
                    "evidence_refs": ["inspector#json:/0"],
                    "fallback_used": false,
                    "join_grade": "weak",
                    "join_key": "tool_call_id",
                    "join_value": null,
                    "left_artifact_role": "external_receipt",
                    "notes": [],
                    "right_artifact_role": "external_receipt",
                    "schema": "assay.observability.join_result.v0",
                    "scope": "tool_call",
                    "unique_within_scope": false,
                },
                "id": "join-1",
                "left": inspector_ref,
                "proposition": {
                    "expected": null,
                    "kind": "equal_retained_value",
                    "left_pointer": "/message/params/arguments/_meta/tool_call_id",
                    "right_pointer": "/message/params/arguments/_meta/tool_call_id",
                    "surface": "transcript_join",
                },
                "result": "unobservable",
                "right": inspector_ref,
            });
            assessment["claims"] = serde_json::json!([]);
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

fn assert_diagnostic_equality(
    value: serde_json::Value,
    remove_left: bool,
    remove_right: bool,
    expected_outcome: IncidentOutcome,
) {
    let (base_package, context) = build_join_claim_package(ClaimScenario::Positive);
    let mut inspector: serde_json::Value =
        serde_json::from_slice(&input_object(&base_package, "inspector")).expect("inspector JSON");
    let first = inspector[0].as_object_mut().expect("first Inspector entry");
    first["message"]["params"]["arguments"]["label"] = value.clone();
    let mut second = serde_json::Value::Object(first.clone());
    second["id"] = serde_json::json!("entry-2");
    second["timestamp"] = serde_json::json!("2026-09-08T00:00:02.500Z");
    second["message"]["id"] = serde_json::json!("rpc-2");
    second["message"]["params"]["arguments"]["_meta"]["tool_call_id"] = serde_json::json!("call-2");
    if remove_left {
        first["message"]["params"]["arguments"]
            .as_object_mut()
            .expect("first arguments")
            .remove("label");
    }
    if remove_right {
        second["message"]["params"]["arguments"]
            .as_object_mut()
            .expect("second arguments")
            .remove("label");
    }
    inspector
        .as_array_mut()
        .expect("Inspector array")
        .push(second);
    let mut inspector_bytes = assay_canonical::jcs::to_vec(&inspector).expect("Inspector JCS");
    inspector_bytes.push(b'\n');
    let package = replace_input_object(
        &base_package,
        "inspector",
        inspector_bytes,
        |assessment, inspector_sha| {
            let units = assessment["units"].as_array_mut().expect("units");
            let first_index = units
                .iter()
                .position(|unit| unit["source"]["input_id"] == "inspector")
                .expect("Inspector unit");
            units[first_index]["result"]["value"]["value"] = inspector[0].clone();
            let mut second_unit = units[first_index].clone();
            second_unit["source"]["locator"] = serde_json::json!("json:/1");
            second_unit["result"]["value"]["source"]["locator"] = serde_json::json!("json:/1");
            second_unit["result"]["value"]["value"] = inspector[1].clone();
            units.push(second_unit);

            let first_ref = serde_json::json!({
                "input_id": "inspector",
                "locator": "json:/0",
                "sha256": inspector_sha,
            });
            let second_ref = serde_json::json!({
                "input_id": "inspector",
                "locator": "json:/1",
                "sha256": inspector_sha,
            });
            assessment["joins"][0] = serde_json::json!({
                "contract": {
                    "evidence_refs": [
                        format!("inspector#json:/0"),
                        format!("inspector#json:/1"),
                    ],
                    "fallback_used": false,
                    "join_grade": "diagnostic",
                    "join_key": "timestamp_or_order",
                    "join_value": value,
                    "left_artifact_role": "external_receipt",
                    "notes": [],
                    "right_artifact_role": "external_receipt",
                    "schema": "assay.observability.join_result.v0",
                    "scope": "diagnostic",
                    "unique_within_scope": false,
                },
                "id": "join-1",
                "left": first_ref,
                "proposition": {
                    "expected": null,
                    "kind": "equal_retained_value",
                    "left_pointer": "/message/params/arguments/label",
                    "right_pointer": "/message/params/arguments/label",
                    "surface": "transcript_join",
                },
                "result": "unobservable",
                "right": second_ref,
            });
            assessment["claims"] = serde_json::json!([]);
            let transcript = assessment["cap1"]["strata"]
                .as_array_mut()
                .expect("CAP-1 strata")
                .iter_mut()
                .find(|stratum| stratum["id"] == "transcript_join")
                .expect("transcript stratum");
            transcript["eligible"] = serde_json::json!(2);
            transcript["examined"] = serde_json::json!(2);
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.outcome, expected_outcome, "{report:?}");
}

#[test]
fn empty_diagnostic_values_remain_unobservable() {
    assert_diagnostic_equality(
        serde_json::json!(""),
        false,
        false,
        IncidentOutcome::PackageVerified,
    );
}

#[test]
fn null_diagnostic_values_remain_unobservable() {
    assert_diagnostic_equality(
        serde_json::Value::Null,
        false,
        false,
        IncidentOutcome::PackageVerified,
    );
}

#[test]
fn missing_diagnostic_pointer_is_not_null() {
    for (remove_left, remove_right) in [(true, false), (false, true)] {
        assert_diagnostic_equality(
            serde_json::Value::Null,
            remove_left,
            remove_right,
            IncidentOutcome::PackageRefused,
        );
    }
}

#[test]
fn non_string_tool_identity_cannot_form_a_strong_join() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("true"),
        serde_json::json!(true),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        None,
        |_| {},
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn selected_left_outside_the_window_cannot_form_a_strong_join() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("call-1"),
        serde_json::json!("call-1"),
        "2026-09-08T00:00:10Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        Some("2026-09-08T00:00:01Z"),
        |_| {},
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn proxy_fallback_identity_cannot_form_a_strong_join() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("req_123"),
        serde_json::json!("req_123"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        None,
        |_| {},
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn equal_retained_right_outside_the_window_cannot_form_a_strong_join() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("call-1"),
        serde_json::json!("call-1"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:10Z",
        "list_files",
        None,
        |assessment| {
            assessment["joins"][0]["proposition"]["kind"] =
                serde_json::json!("equal_retained_value");
            assessment["joins"][0]["proposition"]["expected"] = serde_json::Value::Null;
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn join_requires_the_nullable_right_member_to_be_present() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("call-1"),
        serde_json::json!("call-1"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "list_files",
        None,
        |assessment| {
            assessment["joins"][0]
                .as_object_mut()
                .expect("join object")
                .remove("right");
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::InputShape));
}

#[test]
fn inspector_tool_call_requires_a_nonblank_tool_name() {
    let (package, context) = build_join_claim_package_with_source_options(
        ClaimScenario::Positive,
        false,
        false,
        serde_json::json!("call-1"),
        serde_json::json!("call-1"),
        "2026-09-08T00:00:01Z",
        "2026-09-08T00:00:02Z",
        "",
        None,
        |_| {},
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::InputShape));
}

#[test]
fn recorded_join_result_cannot_authorize_itself() {
    let (package, context) =
        build_join_claim_package_with_mutation(ClaimScenario::Positive, |assessment| {
            assessment["joins"][0]["result"] = serde_json::json!("contradicted")
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn dangling_claim_proposition_reaches_claim_boundary() {
    let (package, context) =
        build_join_claim_package_with_mutation(ClaimScenario::Positive, |assessment| {
            assessment["claims"][0]["proposition"] = serde_json::json!("join-missing")
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn missing_negative_assertion_derives_blocked_without_laundering_support() {
    let (package, context) =
        build_join_claim_package_with_mutation(ClaimScenario::BoundedNegative, |assessment| {
            assessment["cap1"]
                .as_object_mut()
                .expect("CAP-1 object")
                .remove("absence_assertions");
            assessment["claims"][0]["decision"] = serde_json::json!("blocked");
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn orphan_negative_assertion_refuses_independent_of_recorded_decision() {
    let (package, context) =
        build_join_claim_package_with_mutation(ClaimScenario::BoundedNegative, |assessment| {
            assessment["cap1"]["absence_assertions"][0]["assertion"] =
                serde_json::json!("claim-missing");
            assessment["claims"][0]["decision"] = serde_json::json!("blocked");
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn partial_correlation_blocks_binding_join_without_lowering_transcript_surface() {
    let (package, context) =
        build_join_claim_package_with_options(ClaimScenario::Positive, true, false, |assessment| {
            assessment["joins"][0]["contract"]["join_grade"] = serde_json::json!("weak");
            assessment["joins"][0]["contract"]["unique_within_scope"] = serde_json::json!(false);
            assessment["joins"][0]["result"] = serde_json::json!("unobservable");
            assessment["claims"][0]["decision"] = serde_json::json!("blocked");
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn partial_correlation_rejects_a_recorded_strong_binding_join() {
    let (package, context) =
        build_join_claim_package_with_options(ClaimScenario::Positive, true, false, |_| {});
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn degraded_enforcement_preserves_positive_presence_support() {
    let (package, context) =
        build_join_claim_package_with_options(ClaimScenario::Positive, false, true, |_| {});
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn degraded_enforcement_blocks_negative_support_without_lowering_observation() {
    let (package, context) = build_join_claim_package_with_options(
        ClaimScenario::BoundedNegative,
        false,
        true,
        |assessment| {
            assessment["cap1"]
                .as_object_mut()
                .expect("CAP-1 object")
                .remove("absence_assertions");
            assessment["claims"][0]["decision"] = serde_json::json!("blocked");
        },
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn degraded_enforcement_makes_a_negative_assertion_surplus() {
    let (package, context) = build_join_claim_package_with_options(
        ClaimScenario::BoundedNegative,
        false,
        true,
        |assessment| assessment["claims"][0]["decision"] = serde_json::json!("blocked"),
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ClaimBoundary));
}

#[test]
fn fresh_inline_operation_is_checked_at_disclosure_phase() {
    let (package, context) =
        build_join_claim_package_with_mutation(ClaimScenario::Positive, |assessment| {
            assessment["units"][0]["result"]["value"]["operation"] =
                serde_json::json!("wrong-operation");
        });
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::Disclosure));
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

fn add_context_source_target(package: &[u8], id: &str, format: &str, bytes: Vec<u8>) -> Vec<u8> {
    let members = read_test_tar_members(package);
    let mut inventory: serde_json::Value = serde_json::from_slice(
        &members
            .iter()
            .find(|(name, _)| name == "inventory.json")
            .expect("inventory")
            .1,
    )
    .expect("inventory JSON");
    let assessment = members
        .iter()
        .find(|(name, _)| name == "assessment.json")
        .expect("assessment")
        .1
        .clone();
    let sha = hex::encode(Sha256::digest(&bytes));
    inventory["inputs"]
        .as_array_mut()
        .expect("inputs")
        .push(serde_json::json!({
            "binding": null,
            "bytes": bytes.len(),
            "format": format,
            "id": id,
            "sha256": sha,
            "surfaces": [],
        }));
    inventory["inputs"]
        .as_array_mut()
        .expect("inputs")
        .sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).expect("inventory JCS");
    inventory_bytes.push(b'\n');
    let mut objects = members
        .into_iter()
        .filter(|(name, _)| name.starts_with("objects/"))
        .collect::<Vec<_>>();
    objects.push((format!("objects/{sha}"), bytes));
    build_incident_package_tar(&inventory_bytes, &assessment, objects)
}

fn build_context_source_ref_package(
    entries: serde_json::Value,
    extra: Option<(&str, &str, Vec<u8>)>,
) -> (Vec<u8>, ContextInput) {
    let declaration = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.incident.context.v1",
        "entries": entries,
    }))
    .expect("context declaration");
    let (package, context, _) = build_context_basis_package(
        Some(("incident-context-v1", declaration, "json:/entries/0")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    let package = match extra {
        Some((id, format, bytes)) => add_context_source_target(&package, id, format, bytes),
        None => package,
    };
    (package, context)
}

fn context_source_ref(input_id: &str, sha256: &str, locator: &str) -> serde_json::Value {
    serde_json::json!({
        "input_id": input_id,
        "locator": locator,
        "sha256": sha256,
    })
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
fn selected_health_byte_limit_refuses_before_context_digest_comparison() {
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
    let selected_limit = health.len() as u64 - 1;
    let (package, mut context, _) = build_context_basis_package(
        Some(("assay-health-json", health, "json:")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    context.limits.health_bytes = selected_limit;
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ResourceSelected));
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
fn enforcement_health_v1_requires_the_probe_member() {
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
    assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(report.reason, Some(IncidentReason::InputShape));
}

#[test]
fn unreferenced_unknown_health_schema_refuses_admission() {
    let health = serde_json::to_vec(&serde_json::json!({"schema": "assay.health.future.v9"}))
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
    assert_eq!(report.reason, Some(IncidentReason::UnknownSchema));
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

#[test]
fn array_index_locators_reject_plus_aliases_and_preserve_canonical_indices() {
    let declaration = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.incident.context.v1",
        "entries": [
            {"kind": "retention", "value": "first", "source_refs": []},
            {"kind": "retention", "value": "second", "source_refs": []},
        ]
    }))
    .expect("context declaration");
    for (locator, expected) in [
        ("json:/entries/0", IncidentOutcome::PackageVerified),
        ("json:/entries/1", IncidentOutcome::PackageVerified),
        ("json:/entries/+0", IncidentOutcome::PackageRefused),
        ("json:/entries/+1", IncidentOutcome::PackageRefused),
    ] {
        let (package, context, _) = build_context_basis_package(
            Some(("incident-context-v1", declaration.clone(), locator)),
            true,
            "filesystem",
            ("adequate", "adequate", "unique"),
            "adequate",
        );
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.outcome, expected, "{locator}: {report:?}");
        if expected == IncidentOutcome::PackageRefused {
            assert_eq!(
                report.reason,
                Some(IncidentReason::Locator),
                "{locator}: {report:?}"
            );
        }
    }

    let (base_package, context) = build_join_claim_package(ClaimScenario::Positive);
    let inspector_sha = hex::encode(Sha256::digest(input_object(&base_package, "inspector")));
    for (locator, expected) in [
        ("json:/0", IncidentOutcome::PackageVerified),
        ("json:/+0", IncidentOutcome::PackageRefused),
    ] {
        let context_document = serde_json::to_vec(&serde_json::json!({
            "schema": "assay.incident.context.v1",
            "entries": [{
                "kind": "retention",
                "value": "bounded",
                "source_refs": [context_source_ref("inspector", &inspector_sha, locator)],
            }],
        }))
        .expect("context declaration");
        let package = add_context_source_target(
            &base_package,
            "context-alias",
            "incident-context-v1",
            context_document,
        );
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.outcome, expected, "{locator}: {report:?}");
        if expected == IncidentOutcome::PackageRefused {
            assert_eq!(
                report.reason,
                Some(IncidentReason::Locator),
                "{locator}: {report:?}"
            );
        }
    }
}

#[test]
fn incident_context_source_ref_resolves_original_input() {
    let policy = b"version: 1\n".to_vec();
    let sha = hex::encode(Sha256::digest(&policy));
    let (package, context) = build_context_source_ref_package(
        serde_json::json!([{
            "kind": "retention",
            "value": "bounded",
            "source_refs": [context_source_ref("policy", &sha, "bytes:0:11")],
        }]),
        Some(("policy", "policy-bytes", policy)),
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(
        report.outcome,
        IncidentOutcome::PackageVerified,
        "{report:?}"
    );
}

#[test]
fn incident_context_source_ref_rejects_missing_or_invalid_target() {
    let policy = b"version: 1\n".to_vec();
    let sha = hex::encode(Sha256::digest(&policy));
    for reference in [
        context_source_ref("missing", &sha, "bytes:0:11"),
        context_source_ref("policy", &"0".repeat(64), "bytes:0:11"),
        context_source_ref("policy", &sha, "bytes:0:12"),
    ] {
        let (package, context) = build_context_source_ref_package(
            serde_json::json!([{
                "kind": "retention",
                "value": "bounded",
                "source_refs": [reference],
            }]),
            Some(("policy", "policy-bytes", policy.clone())),
        );
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.reason, Some(IncidentReason::Locator), "{report:?}");
    }
}

#[test]
fn incident_context_checks_unselected_entry_source_refs() {
    let (package, context) = build_context_source_ref_package(
        serde_json::json!([
            {"kind": "retention", "value": "bounded", "source_refs": []},
            {
                "kind": "actor",
                "value": "operator supplied",
                "source_refs": [context_source_ref("missing", &"0".repeat(64), "json:")],
            },
        ]),
        None,
    );
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::Locator), "{report:?}");
}

#[test]
fn incident_context_source_ref_rejects_context_and_result_chains() {
    for (id, format, bytes, locator) in [
        (
            "nested-context",
            "incident-context-v1",
            br#"{"schema":"assay.incident.context.v1","entries":[{"kind":"retention","value":"bounded","source_refs":[]}] }"#
                .to_vec(),
            "json:/entries/0",
        ),
        (
            "nested-results",
            "incident-results-v1",
            br#"{"schema":"assay.incident.results.v1","results":[{"operation":"policy-bytes","source":{"input_id":"policy","sha256":"0000000000000000000000000000000000000000000000000000000000000000","locator":"bytes:0:0"},"value":null}]}"#
                .to_vec(),
            "json:/results/0",
        ),
    ] {
        let sha = hex::encode(Sha256::digest(&bytes));
        let (package, context) = build_context_source_ref_package(
            serde_json::json!([{
                "kind": "retention",
                "value": "bounded",
                "source_refs": [context_source_ref(id, &sha, locator)],
            }]),
            Some((id, format, bytes)),
        );
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.reason, Some(IncidentReason::Locator), "{report:?}");
    }
}

#[test]
fn incident_context_source_refs_charge_the_reference_limit() {
    let declaration = serde_json::to_vec(&serde_json::json!({
        "schema": "assay.incident.context.v1",
        "entries": [{
            "kind": "retention",
            "value": "bounded",
            "source_refs": ["retained:one", "retained:two"]
        }]
    }))
    .expect("context declaration");
    let (package, mut context, _) = build_context_basis_package(
        Some(("incident-context-v1", declaration, "json:/entries/0")),
        true,
        "filesystem",
        ("adequate", "adequate", "unique"),
        "adequate",
    );
    context.limits.references = 1;
    let report = verify_incident_package(&package, &context);
    assert_eq!(report.reason, Some(IncidentReason::ResourceSelected));
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

#[test]
fn policy_and_key_budgets_cover_unused_opaque_inputs() {
    let base = read_fixture("valid_present_empty.tar");
    for (format, hard) in [
        ("policy-bytes", 1_048_576),
        ("ed25519-public-key-pem", 16_384),
    ] {
        for (size, selected, reason) in [
            (hard, hard, None),
            (4, 4, None),
            (hard + 1, hard, Some(IncidentReason::ResourceHard)),
            (5, 4, Some(IncidentReason::ResourceSelected)),
            (hard + 1, 4, Some(IncidentReason::ResourceHard)),
        ] {
            let mut context = ContextInput::default();
            if format == "policy-bytes" {
                context.limits.policy_bytes = selected as u64;
            } else {
                context.limits.key_bytes = selected as u64;
            }
            // Unused objects remain opaque: UTF-8/PEM validity is not a preflight requirement.
            let package = add_context_source_target(&base, "unused", format, vec![0xff; size]);
            let package = rebind_package_context(&package, &context);
            let report = verify_incident_package(&package, &context);
            assert_eq!(
                report.reason, reason,
                "{format}, size={size}, selected={selected}"
            );
            assert_eq!(
                report.outcome,
                if reason.is_none() {
                    IncidentOutcome::PackageVerified
                } else {
                    IncidentOutcome::PackageRefused
                }
            );
        }
    }
}

#[test]
fn policy_and_key_budgets_preserve_earlier_aggregate_and_digest_refusals() {
    let base = read_fixture("valid_present_empty.tar");
    for (format, hard) in [
        ("policy-bytes", 1_048_576),
        ("ed25519-public-key-pem", 16_384),
    ] {
        let package = add_context_source_target(&base, "oversized", format, vec![0xff; hard + 1]);
        let mut context = ContextInput::default();
        context.limits.object_bytes_total = hard as u64;
        assert_eq!(
            verify_incident_package(&package, &context).reason,
            Some(IncidentReason::ResourceSelected)
        );

        let members = read_test_tar_members(&package);
        let mut inventory: serde_json::Value = serde_json::from_slice(
            &members
                .iter()
                .find(|(name, _)| name == "inventory.json")
                .unwrap()
                .1,
        )
        .unwrap();
        // Keep the object's digest-correct tar path intact; contradict its declared length.
        inventory["inputs"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|input| input["id"] == "oversized")
            .unwrap()["bytes"] = serde_json::json!(hard);
        let mut inventory = assay_canonical::jcs::to_vec(&inventory).unwrap();
        inventory.push(b'\n');
        let assessment = members
            .iter()
            .find(|(name, _)| name == "assessment.json")
            .unwrap()
            .1
            .clone();
        let objects = members
            .into_iter()
            .filter(|(name, _)| name.starts_with("objects/"))
            .collect();
        let corrupted = build_incident_package_tar(&inventory, &assessment, objects);
        assert_eq!(
            verify_incident_package(&corrupted, &ContextInput::default()).reason,
            Some(IncidentReason::Digest)
        );
    }
}

#[test]
fn policy_and_key_budgets_preserve_input_id_order() {
    let base = read_fixture("valid_present_empty.tar");
    for (format, hard) in [
        ("policy-bytes", 1_048_576),
        ("ed25519-public-key-pem", 16_384),
    ] {
        let mut context = ContextInput::default();
        if format == "policy-bytes" {
            context.limits.policy_bytes = 4;
        } else {
            context.limits.key_bytes = 4;
        }
        for (first_size, second_size, expected) in [
            (5, hard + 1, IncidentReason::ResourceSelected),
            (hard + 1, 5, IncidentReason::ResourceHard),
        ] {
            // Insert out of order; the helper emits the required canonical input-id order.
            let package = add_context_source_target(&base, "b", format, vec![0xff; second_size]);
            let package = add_context_source_target(&package, "a", format, vec![0xff; first_size]);
            let package = rebind_package_context(&package, &context);
            assert_eq!(
                verify_incident_package(&package, &context).reason,
                Some(expected)
            );
        }
    }
}

#[test]
fn key_budget_precedes_bound_key_parsing_and_trust_selection() {
    let bundle = make_test_bundle("run-key-budget");
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(&bundle).unwrap();
    let envelope = serde_json::to_vec(&sign_statement(&statement, &signing_key).unwrap()).unwrap();
    let pem = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .unwrap()
        .into_bytes();
    let (package, mut context) =
        build_test_package_with_attestation(&envelope, &bundle, &pem, None);
    context.limits.key_bytes = pem.len() as u64;
    let package = rebind_package_context(&package, &context);
    assert_eq!(
        verify_incident_package(&package, &context).outcome,
        IncidentOutcome::PackageVerified
    );
    context.limits.key_bytes -= 1;
    let package = rebind_package_context(&package, &context);
    assert_eq!(
        verify_incident_package(&package, &context).reason,
        Some(IncidentReason::ResourceSelected)
    );

    let (malformed, malformed_context) =
        build_test_package_with_attestation(&envelope, &bundle, &[0xff; 5], None);
    assert_eq!(
        verify_incident_package(&malformed, &malformed_context).reason,
        Some(IncidentReason::InputShape)
    );

    for (size, selected, expected) in [
        (16_385, 16_384, IncidentReason::ResourceHard),
        (5, 4, IncidentReason::ResourceSelected),
    ] {
        let (package, mut context) =
            build_test_package_with_attestation(&envelope, &bundle, &vec![0xff; size], None);
        context.limits.key_bytes = selected;
        for trusted in [true, false] {
            if !trusted {
                context.keys.clear();
            }
            assert_eq!(
                verify_incident_package(&package, &context).reason,
                Some(expected),
                "trusted={trusted}"
            );
        }
    }
}

#[test]
fn outer_path_budget_selected_boundaries() {
    let fixture = read_fixture("valid_present_empty.tar");
    for (limit, reason) in [
        (72, None),
        (80, None),
        (13, Some(IncidentReason::ResourceSelected)),
        (14, Some(IncidentReason::ResourceSelected)),
        (15, Some(IncidentReason::ResourceSelected)),
        (71, Some(IncidentReason::ResourceSelected)),
    ] {
        let mut context = ContextInput::default();
        context.limits.outer_path_bytes = limit;
        let package = rebind_package_context(&fixture, &context);
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.reason, reason, "path limit {limit}: {report:?}");
        assert_eq!(
            report.outcome,
            if reason.is_none() {
                IncidentOutcome::PackageVerified
            } else {
                IncidentOutcome::PackageRefused
            }
        );
    }
}

fn repair_outer_header_checksum(header: &mut [u8]) {
    header[148..156].fill(b' ');
    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

#[test]
fn outer_path_budget_hard_precedes_selected_and_path_grammar() {
    for (length, selected, reason) in [
        (80, 80, IncidentReason::InputShape),
        (81, 80, IncidentReason::ResourceHard),
        (81, 13, IncidentReason::ResourceHard),
        (100, 80, IncidentReason::ResourceHard),
    ] {
        let mut package = read_fixture("valid_present_empty.tar");
        package[..100].fill(0);
        package[..length].fill(b'x');
        repair_outer_header_checksum(&mut package[..512]);
        let mut context = ContextInput::default();
        context.limits.outer_path_bytes = selected;
        assert_eq!(
            verify_incident_package(&package, &context).reason,
            Some(reason),
            "name length {length}, selected {selected}"
        );
        if selected == 80 {
            assert_eq!(
                assay_evidence::read_incident_container(&package)
                    .unwrap_err()
                    .reason,
                Some(reason),
                "public reader name length {length}"
            );
        }
    }
}

#[test]
fn outer_path_budget_preserves_header_and_first_refusal_order() {
    use IncidentReason::{InputShape, ResourceSelected};
    let fixture = read_fixture("valid_present_empty.tar");
    for (mutation, limit, reason) in [
        ("payload", 71, ResourceSelected),
        ("payload", 72, InputShape),
        ("checksum", 71, InputShape),
        ("prefix", 71, InputShape),
        ("utf8", 71, ResourceSelected),
        ("utf8", 80, InputShape),
        ("nul_tail", 13, ResourceSelected),
        ("nul_tail", 14, InputShape),
        ("nul_tail", 80, InputShape),
        ("padding", 71, ResourceSelected),
        ("padding", 72, InputShape),
        ("truncated", 71, ResourceSelected),
        ("truncated", 72, InputShape),
    ] {
        let mut context = ContextInput::default();
        context.limits.outer_path_bytes = limit;
        let mut package = rebind_package_context(&fixture, &context);
        let members = read_test_tar_members(&package);
        let object_header: usize = members
            .iter()
            .take(2)
            .map(|(_, data)| 512 + data.len().div_ceil(512) * 512)
            .sum();
        match mutation {
            "payload" => package[object_header + 512] ^= 1,
            "checksum" => package[object_header + 148] = b'x',
            "prefix" | "utf8" => {
                let offset = if mutation == "prefix" { 345 } else { 8 };
                package[object_header + offset] = 0xff;
                repair_outer_header_checksum(&mut package[object_header..object_header + 512]);
            }
            "nul_tail" => {
                package[15] = b'X';
                repair_outer_header_checksum(&mut package[..512]);
            }
            "padding" => {
                assert!(!members[2].1.len().is_multiple_of(512));
                package[object_header + 512 + members[2].1.len()] = 1;
            }
            "truncated" => package.truncate(object_header + 512),
            _ => unreachable!(),
        }
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
        assert_eq!(
            report.reason,
            Some(reason),
            "{mutation}, limit {limit}: {report:?}"
        );
    }
}

#[test]
fn outer_path_budget_invalid_context_precedes_container() {
    for limit in [0, 81] {
        let mut context = ContextInput::default();
        context.limits.outer_path_bytes = limit;
        for package in [read_fixture("valid_present_empty.tar"), vec![]] {
            assert_eq!(
                verify_incident_package(&package, &context).reason,
                Some(IncidentReason::TrustInput)
            );
        }
    }
}

#[test]
fn member_budget_selected_equality_and_required_members() {
    for (limit, reason) in [
        (1, Some(IncidentReason::ResourceSelected)),
        (2, Some(IncidentReason::ResourceSelected)),
        (3, None),
    ] {
        let mut context = ContextInput::default();
        context.limits.members = limit;
        let package = rebind_package_context(&read_fixture("valid_present_empty.tar"), &context);
        let report = verify_incident_package(&package, &context);
        assert_eq!(report.reason, reason, "limit {limit}: {report:?}");
        assert_eq!(
            report.outcome,
            if reason.is_none() {
                IncidentOutcome::PackageVerified
            } else {
                IncidentOutcome::PackageRefused
            }
        );
    }
}

#[test]
fn member_budget_precedes_header_path_and_payload_processing() {
    use IncidentReason::{InputShape, ResourceHard, ResourceSelected};
    for (mutation, admitted_reason) in [
        ("payload", InputShape),
        ("checksum", InputShape),
        ("prefix", InputShape),
        ("truncated_payload", InputShape),
        ("path", ResourceHard),
        ("partial_header", InputShape),
        ("earlier_header", InputShape),
    ] {
        for limit in [2, 3] {
            let mut context = ContextInput::default();
            context.limits.members = limit;
            let mut package =
                rebind_package_context(&read_fixture("valid_present_empty.tar"), &context);
            let members = read_test_tar_members(&package);
            let offset: usize = members
                .iter()
                .take(2)
                .map(|(_, data)| 512 + data.len().div_ceil(512) * 512)
                .sum();
            match mutation {
                "payload" => package[offset + 512] ^= 1,
                "checksum" => package[offset + 148] = b'x',
                "prefix" => {
                    package[offset + 345] = 1;
                    repair_outer_header_checksum(&mut package[offset..offset + 512]);
                }
                "path" => {
                    package[offset..offset + 100].fill(0);
                    package[offset..offset + 81].fill(b'x');
                    repair_outer_header_checksum(&mut package[offset..offset + 512]);
                }
                "truncated_payload" => package.truncate(offset + 512),
                "partial_header" => package.truncate(offset + 100),
                "earlier_header" => package[148] = b'x',
                _ => unreachable!(),
            }
            let reason = if limit == 2 && !matches!(mutation, "partial_header" | "earlier_header") {
                ResourceSelected
            } else {
                admitted_reason
            };
            let report = verify_incident_package(&package, &context);
            assert_eq!(report.outcome, IncidentOutcome::PackageRefused);
            assert_eq!(
                report.reason,
                Some(reason),
                "{mutation}, limit {limit}: {report:?}"
            );
        }
    }
}

#[test]
fn member_budget_invalid_context_precedes_container() {
    for limit in [0, 129] {
        let mut context = ContextInput::default();
        context.limits.members = limit;
        for bytes in [read_fixture("valid_present_empty.tar"), vec![]] {
            assert_eq!(
                verify_incident_package(&bytes, &context).reason,
                Some(IncidentReason::TrustInput)
            );
        }
    }
}

fn mutate_inventory(package: &[u8], mutate: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let members = read_test_tar_members(package);
    let mut inventory: serde_json::Value = serde_json::from_slice(&members[0].1).unwrap();
    mutate(&mut inventory);
    let mut bytes = assay_canonical::jcs::to_vec(&inventory).unwrap();
    bytes.push(b'\n');
    build_incident_package_tar(&bytes, &members[1].1, members[2..].to_vec())
}

#[test]
fn inventory_context_requires_closed_disclosure_grammar() {
    use serde_json::json;
    let original = read_fixture("valid_present_empty.tar");
    assert_eq!(
        verify_incident_package(&original, &ContextInput::default()).outcome,
        IncidentOutcome::PackageVerified
    );
    let mut cases = vec![json!(null), json!([]), json!({})];
    let base = empty_incident_context();
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("actor");
    cases.push(missing);
    let mut extra = base.clone();
    extra["extra"] = json!({"value":null,"reason":"not_available","refs":[]});
    cases.push(extra);
    for entry in [
        json!({"reason":"stated","refs":[],"value":"an assertion"}),
        json!({"reason":"stated","refs":[],"value":null}),
        json!({"reason":"unknown","refs":[],"value":null}),
        json!({"reason":"not_available","refs":[],"value":"an assertion"}),
        json!({"reason":"not_applicable","refs":[],"value":false}),
        json!({"reason":"not_available","refs":[]}),
        json!({"reason":"not_available","value":null}),
        json!({"refs":[],"value":null}),
        json!({"reason":"not_available","refs":[],"value":null,"extra":true}),
        json!({"reason":"stated","refs":[{"input_id":"x","sha256":"0".repeat(64)}],"value":"x"}),
    ] {
        let mut context = base.clone();
        context["actor"] = entry;
        cases.push(context);
    }
    for (index, invalid) in cases.into_iter().enumerate() {
        let package = mutate_inventory(&original, |i| i["context"] = invalid);
        let report = verify_incident_package(&package, &ContextInput::default());
        assert_eq!(
            report.reason,
            Some(IncidentReason::InputShape),
            "case {index}: {report:?}"
        );
    }
}

fn inventory_context_reference_package(kind: &str) -> Vec<u8> {
    let declaration = serde_json::to_vec(&serde_json::json!({
        "schema":"assay.incident.context.v1",
        "entries":[{"kind":kind,"value":"operator declaration","source_refs":[]}]
    }))
    .unwrap();
    let digest = hex::encode(Sha256::digest(&declaration));
    let package = add_context_source_target(
        &read_fixture("valid_present_empty.tar"),
        "declaration",
        "incident-context-v1",
        declaration,
    );
    mutate_inventory(&package, |i| {
        i["context"]["actor"] = serde_json::json!({
            "reason":"stated","value":"operator declaration",
            "refs":[context_source_ref("declaration", &digest, "json:/entries/0")]
        });
    })
}

#[test]
fn inventory_context_resolves_retained_references_and_matching_kind() {
    let package = inventory_context_reference_package("actor");
    let ctx = ContextInput::default();
    assert_eq!(
        verify_incident_package(&package, &ctx).outcome,
        IncidentOutcome::PackageVerified
    );
    for (field, bad) in [
        ("input_id", "missing".to_string()),
        ("sha256", "0".repeat(64)),
        ("locator", "json:/entries/1".to_string()),
    ] {
        let invalid = mutate_inventory(&package, |i| {
            i["context"]["actor"]["refs"][0][field] = serde_json::json!(bad);
        });
        assert_eq!(
            verify_incident_package(&invalid, &ctx).reason,
            Some(IncidentReason::Locator),
            "{field}"
        );
    }
    assert_eq!(
        verify_incident_package(&inventory_context_reference_package("clock"), &ctx).reason,
        Some(IncidentReason::Locator),
        "context entry kind must match disclosure kind"
    );
}

fn inventory_policy_reference_package() -> Vec<u8> {
    let policy = b"version: '1'\n".to_vec();
    let sha = hex::encode(Sha256::digest(&policy));
    let locator = format!("bytes:0:{}", policy.len());
    let package = add_context_source_target(
        &read_fixture("valid_present_empty.tar"),
        "policy",
        "policy-bytes",
        policy,
    );
    mutate_inventory(&package, |i| {
        i["context"]["policy_activation"] = serde_json::json!({
            "reason":"stated", "value":"operator-selected policy bytes; no activation proof",
            "refs":[context_source_ref("policy", &sha, &locator)]
        });
    })
}

#[test]
fn inventory_policy_reference_checks_digest_and_whole_byte_locator() {
    let package = inventory_policy_reference_package();
    let ctx = ContextInput::default();
    assert_eq!(
        verify_incident_package(&package, &ctx).outcome,
        IncidentOutcome::PackageVerified
    );
    for (field, value) in [
        ("sha256", "0".repeat(64)),
        ("locator", "bytes:0:1".into()),
        ("locator", "json:".into()),
    ] {
        let bad = mutate_inventory(&package, |i| {
            i["context"]["policy_activation"]["refs"][0][field] = serde_json::json!(value);
        });
        assert_eq!(
            verify_incident_package(&bad, &ctx).reason,
            Some(IncidentReason::PolicyBinding),
            "{field}"
        );
    }
    let missing = mutate_inventory(&package, |i| {
        i["context"]["policy_activation"]["refs"][0]["input_id"] = serde_json::json!("missing");
    });
    assert_eq!(
        verify_incident_package(&missing, &ctx).reason,
        Some(IncidentReason::Locator)
    );
}

#[test]
fn inventory_policy_binding_follows_digest_and_precedes_nonpolicy_locator() {
    let bad_policy = mutate_inventory(&inventory_policy_reference_package(), |i| {
        i["context"]["policy_activation"]["refs"][0]["sha256"] = serde_json::json!("0".repeat(64));
        i["context"]["actor"] = serde_json::json!({"reason":"stated", "value":"missing",
            "refs":[context_source_ref("missing", &"0".repeat(64), "json:")]});
    });
    let ctx = ContextInput::default();
    assert_eq!(
        verify_incident_package(&bad_policy, &ctx).reason,
        Some(IncidentReason::PolicyBinding)
    );
    let bad_digest = mutate_inventory(&bad_policy, |i| {
        i["assessment_sha256"] = serde_json::json!("0".repeat(64));
    });
    assert_eq!(
        verify_incident_package(&bad_digest, &ctx).reason,
        Some(IncidentReason::Digest)
    );
}

#[test]
fn inventory_context_reference_budget_is_inclusive() {
    let package = inventory_policy_reference_package();
    let mut ctx = ContextInput::default();
    ctx.limits.references = 1;
    let exact = rebind_package_context(&package, &ctx);
    assert_eq!(
        verify_incident_package(&exact, &ctx).outcome,
        IncidentOutcome::PackageVerified
    );
    let over = mutate_inventory(&exact, |i| {
        let reference = i["context"]["policy_activation"]["refs"][0].clone();
        i["context"]["policy_activation"]["refs"]
            .as_array_mut()
            .unwrap()
            .push(reference);
    });
    assert_eq!(
        verify_incident_package(&over, &ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );
}

#[test]
fn assessment_policy_reference_is_checked_before_generic_locator() {
    let package = inventory_policy_reference_package();
    let members = read_test_tar_members(&package);
    let inventory: serde_json::Value = serde_json::from_slice(&members[0].1).unwrap();
    let reference = inventory["context"]["policy_activation"]["refs"][0].clone();
    // A valid byte binding does not make a policy an observation-basis source.
    for (bad_binding, reason) in [
        (false, IncidentReason::Locator),
        (true, IncidentReason::PolicyBinding),
    ] {
        let mut assessment: serde_json::Value = serde_json::from_slice(&members[1].1).unwrap();
        let mut reference = reference.clone();
        if bad_binding {
            reference["sha256"] = serde_json::json!("0".repeat(64));
        }
        assessment["surfaces"][0]["basis_refs"] = serde_json::json!([reference]);
        let mut assessment_bytes = assay_canonical::jcs::to_vec(&assessment).unwrap();
        assessment_bytes.push(b'\n');
        let mut inventory = inventory.clone();
        inventory["assessment_sha256"] =
            serde_json::json!(hex::encode(Sha256::digest(&assessment_bytes)));
        let mut inventory_bytes = assay_canonical::jcs::to_vec(&inventory).unwrap();
        inventory_bytes.push(b'\n');
        let package =
            build_incident_package_tar(&inventory_bytes, &assessment_bytes, members[2..].to_vec());
        assert_eq!(
            verify_incident_package(&package, &ContextInput::default()).reason,
            Some(reason)
        );
    }
}

#[test]
fn inventory_policy_activation_may_reference_a_context_declaration() {
    let package = inventory_context_reference_package("policy_activation");
    let package = mutate_inventory(&package, |i| {
        let declaration = i["context"]["actor"].clone();
        i["context"]["actor"] =
            serde_json::json!({"value":null,"reason":"not_applicable","refs":[]});
        i["context"]["policy_activation"] = declaration;
    });
    assert_eq!(
        verify_incident_package(&package, &ContextInput::default()).outcome,
        IncidentOutcome::PackageVerified
    );
}
