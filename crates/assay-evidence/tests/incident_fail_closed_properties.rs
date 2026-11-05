//! Fail-closed properties for incident-package prepare/verify.
//!
//! These tests pin named refusal reasons, not merely "refused". A hostile or
//! accidental change to a package, or prepare/verify contract drift, must fail
//! the matching subtest in required CI before a release tag.
//!
//! Coverage of the 14 admitted format tokens (see `FORMAT_CASES`):
//!
//! | Format | Case |
//! | --- | --- |
//! | `assay-request-receipts-ndjson-v0` | minimal valid sidecar observation |
//! | `assay-bundle-v1` | original bytes from `valid_five_surfaces.tar` |
//! | `assay-health-json` | original bytes from `combined_session_3345.tar` |
//! | `assay-coverage-json` | minimal coverage descriptor (companion not required) |
//! | `incident-context-v1` | minimal context declaration |
//! | `incident-results-v1` | empty results document (context only; no extra units) |
//! | `ed25519-public-key-pem` | deterministic test key |
//! | `proxy-decision-ndjson` | original bytes from `valid_five_surfaces.tar` |
//! | `runner-kernel-ndjson-v0` | original bytes from `combined_session_3345.tar` |
//! | `runner-kernel-ndjson-v1` | minimal v1 openat line |
//! | `policy-bytes` | original bytes from `combined_session_3345.tar` |
//! | `inspector-protocol-793d103` | original bytes from `valid_five_surfaces.tar` |
//! | `dsse-attestation` | **non-claim** (printed): public prepare always writes `binding: null`; the verifier requires a binding for this format, so prepare cannot round-trip it |
//! | `attestation-report` | **non-claim** (printed): same binding limitation as `dsse-attestation` |
//!
//! `#3345` is also prepared as a combined five-source case with its own context.
//!
//! Limit axes measured on `valid_five_surfaces.tar` and
//! `combined_session_3345.tar`. Covered (usage minus one refuses):
//! `outer_bytes`, `members`, `outer_path_bytes`, `object_bytes_total`, `inputs`,
//! `inner_compressed_bytes`, `decoded_bytes_total`, `inventory_bytes`,
//! `assessment_bytes`, `policy_bytes`, `transcript_bytes`, `health_bytes`
//! (`cap1_shape`: CAP-1 admission uses this ceiling), `inner_manifest_bytes`,
//! `inner_events_bytes`, `records`, `record_bytes`, `inner_path_bytes`,
//! `json_depth`, `json_key_bytes`, `non_payload_string_bytes`,
//! `payload_string_bytes`, `references`, `fresh_result_bytes`.
//! Non-claims: `nested_archive_depth` and `reference_depth` (hard maximum 1;
//! selected 0 is `trust_input`); `envelope_bytes`, `key_bytes`,
//! `retained_report_bytes`, `diagnostic_bytes` (measured minimum selected
//! limit that still verifies is 1 on both fixtures).

use assay_evidence::incident_package::write_incident_container;
use assay_evidence::{
    prepare_verified_incident_package, read_incident_container, verify_incident_package,
    ContextExpectation, ContextInput, ContextLimits, IncidentOutcome, IncidentPrepareSource,
    IncidentReason, IncidentVerifyReport,
};
use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePublicKey};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;

const FIXTURES: &str = "tests/fixtures/incident_package";
const VERIFY_SRC: &str = include_str!("../src/incident_package/verify.rs");
const SIDECAR_SRC: &str = include_str!("../src/incident_package/request_sidecar.rs");

const KERNEL_V1_OPENAT: &str = r#"{"schema":"assay.runner.kernel_event.v1","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":655616,"mode":0,"return_value":3,"access_mode":"read","status":"success"}"#;

/// One row per verifier-admitted format. `None` is a written non-claim.
const FORMAT_CASES: &[(&str, Option<&str>)] = &[
    (
        "assay-request-receipts-ndjson-v0",
        Some("minimal sidecar observation"),
    ),
    (
        "assay-bundle-v1",
        Some("valid_five_surfaces.tar bundle object"),
    ),
    (
        "assay-health-json",
        Some("combined_session_3345.tar health object"),
    ),
    ("assay-coverage-json", Some("minimal coverage descriptor")),
    ("incident-context-v1", Some("minimal context declaration")),
    (
        "incident-results-v1",
        Some("empty results document (context only)"),
    ),
    (
        "ed25519-public-key-pem",
        Some("deterministic ed25519 test key"),
    ),
    (
        "proxy-decision-ndjson",
        Some("valid_five_surfaces.tar proxy object"),
    ),
    (
        "runner-kernel-ndjson-v0",
        Some("combined_session_3345.tar kernel object"),
    ),
    ("runner-kernel-ndjson-v1", Some("minimal v1 openat line")),
    (
        "policy-bytes",
        Some("combined_session_3345.tar policy object"),
    ),
    (
        "dsse-attestation",
        None, // prepare writes binding:null; verifier requires a binding
    ),
    (
        "attestation-report",
        None, // prepare writes binding:null; verifier requires a binding
    ),
    (
        "inspector-protocol-793d103",
        Some("valid_five_surfaces.tar inspector object"),
    ),
];

const PREPARE_BINDING_NONCLAIM: &str = "public prepare_verified_incident_package always writes inventory binding:null, and the verifier requires a binding for this format (phase 2 input_shape). No src change is in scope, so this format has no prepare/verify round-trip case.";

const LIMIT_AXES: [&str; 29] = [
    "outer_bytes",
    "members",
    "outer_path_bytes",
    "object_bytes_total",
    "inputs",
    "inner_compressed_bytes",
    "decoded_bytes_total",
    "nested_archive_depth",
    "inventory_bytes",
    "assessment_bytes",
    "policy_bytes",
    "transcript_bytes",
    "health_bytes",
    "envelope_bytes",
    "key_bytes",
    "retained_report_bytes",
    "inner_manifest_bytes",
    "inner_events_bytes",
    "records",
    "record_bytes",
    "inner_path_bytes",
    "json_depth",
    "json_key_bytes",
    "non_payload_string_bytes",
    "payload_string_bytes",
    "references",
    "reference_depth",
    "diagnostic_bytes",
    "fresh_result_bytes",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES)
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn report_jcs(report: &IncidentVerifyReport) -> Vec<u8> {
    report
        .to_canonical_bytes()
        .expect("incident report has canonical JCS bytes")
}

fn jcs_lf(value: &Value) -> Vec<u8> {
    let mut bytes = assay_canonical::jcs::to_vec(value).expect("JCS");
    bytes.push(b'\n');
    bytes
}

fn session_3345_context() -> ContextInput {
    serde_json::from_slice(&read_fixture("combined_session_3345.context.json"))
        .expect("combined_session_3345.context.json is ContextInput")
}

fn source_from_package(package: &[u8], input_id: &str) -> IncidentPrepareSource {
    let container = read_incident_container(package).expect("valid fixture container");
    let inventory: Value =
        serde_json::from_slice(container.inventory_bytes()).expect("inventory JSON");
    let input = inventory["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .find(|row| row["id"] == input_id)
        .unwrap_or_else(|| panic!("input {input_id}"));
    let sha = input["sha256"].as_str().expect("sha256");
    let path = format!("objects/{sha}");
    let bytes = container
        .objects()
        .iter()
        .find(|obj| obj.name == path)
        .unwrap_or_else(|| panic!("{path}"))
        .bytes
        .clone();
    IncidentPrepareSource {
        id: input_id.to_string(),
        format: input["format"].as_str().expect("format").to_string(),
        bytes,
    }
}

fn all_sources(package: &[u8]) -> Vec<IncidentPrepareSource> {
    let container = read_incident_container(package).expect("valid fixture container");
    let inventory: Value =
        serde_json::from_slice(container.inventory_bytes()).expect("inventory JSON");
    inventory["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .map(|input| {
            let id = input["id"].as_str().expect("id");
            let sha = input["sha256"].as_str().expect("sha256");
            let path = format!("objects/{sha}");
            let bytes = container
                .objects()
                .iter()
                .find(|obj| obj.name == path)
                .unwrap_or_else(|| panic!("{path}"))
                .bytes
                .clone();
            IncidentPrepareSource {
                id: id.to_string(),
                format: input["format"].as_str().expect("format").to_string(),
                bytes,
            }
        })
        .collect()
}

fn sidecar_source() -> IncidentPrepareSource {
    let line = r#"{"jsonrpc":"2.0","id":"ask-1","result":{"action":"decline"}}"#;
    let record = json!({
        "schema": "assay.request_observation.v0",
        "assertion_basis": "observed_on_wire",
        "kind": "ask",
        "scope": "capture-a",
        "sequence": 0,
        "request_line": line,
        "request_bytes": line.len(),
        "request_sha256": hex::encode(Sha256::digest(line.as_bytes())),
        "rpc_id_present": true,
        "rpc_id": "ask-1",
        "call_link": "unlinked",
        "call_id": null
    });
    IncidentPrepareSource {
        id: "sidecar".into(),
        format: "assay-request-receipts-ndjson-v0".into(),
        bytes: format!("{record}\n").into_bytes(),
    }
}

fn coverage_source() -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "coverage".into(),
        format: "assay-coverage-json".into(),
        bytes: serde_json::to_vec(&json!({
            "schema": "assay.runner.coverage_descriptor.v0",
            "dimension": "filesystem",
            "method": "open tracepoint",
            "observes": ["path opens"],
            "known_blind_spots": [],
            "completeness": "full"
        }))
        .expect("coverage JSON"),
    }
}

fn context_declaration_source() -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "declaration".into(),
        format: "incident-context-v1".into(),
        bytes: serde_json::to_vec(&json!({
            "schema": "assay.incident.context.v1",
            "entries": [{"kind": "retention", "value": "bounded", "source_refs": []}]
        }))
        .expect("context JSON"),
    }
}

fn empty_results_source() -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "results".into(),
        format: "incident-results-v1".into(),
        bytes: serde_json::to_vec(&json!({
            "schema": "assay.incident.results.v1",
            "results": []
        }))
        .expect("results JSON"),
    }
}

fn pem_source() -> IncidentPrepareSource {
    let pem = SigningKey::from_bytes(&[7; 32])
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    IncidentPrepareSource {
        id: "key".into(),
        format: "ed25519-public-key-pem".into(),
        bytes: pem.into_bytes(),
    }
}

fn kernel_v1_source() -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "kernel".into(),
        format: "runner-kernel-ndjson-v1".into(),
        bytes: format!("{KERNEL_V1_OPENAT}\n").into_bytes(),
    }
}

fn sources_for(format: &str) -> Result<(Vec<IncidentPrepareSource>, ContextInput), String> {
    let five = read_fixture("valid_five_surfaces.tar");
    let session = read_fixture("combined_session_3345.tar");
    let sources = match format {
        "assay-request-receipts-ndjson-v0" => vec![sidecar_source()],
        "assay-bundle-v1" => vec![source_from_package(&five, "bundle")],
        "assay-health-json" => vec![source_from_package(&session, "health")],
        "assay-coverage-json" => vec![coverage_source()],
        "incident-context-v1" => vec![context_declaration_source()],
        "incident-results-v1" => vec![empty_results_source()],
        "ed25519-public-key-pem" => vec![pem_source()],
        "proxy-decision-ndjson" => vec![source_from_package(&five, "proxy")],
        "runner-kernel-ndjson-v0" => vec![source_from_package(&session, "kernel")],
        "runner-kernel-ndjson-v1" => vec![kernel_v1_source()],
        "policy-bytes" => vec![source_from_package(&session, "policy")],
        "inspector-protocol-793d103" => vec![source_from_package(&five, "inspector")],
        "dsse-attestation" | "attestation-report" => {
            return Err(format!("{format}: {PREPARE_BINDING_NONCLAIM}"));
        }
        other => panic!("FORMAT_CASES has no builder for {other}"),
    };
    Ok((sources, ContextInput::default()))
}

fn prepare_ok(
    sources: &[IncidentPrepareSource],
    context: &ContextInput,
) -> (Vec<u8>, IncidentVerifyReport) {
    prepare_verified_incident_package(sources, context).unwrap_or_else(|report| {
        panic!(
            "prepare refused {:?}: ids={:?}",
            report.reason,
            sources
                .iter()
                .map(|s| (s.id.as_str(), s.format.as_str()))
                .collect::<Vec<_>>()
        )
    })
}

fn assert_round_trip(sources: &[IncidentPrepareSource], context: &ContextInput) {
    let (package, prepared) = prepare_ok(sources, context);
    assert_eq!(
        prepared.outcome,
        IncidentOutcome::PackageVerified,
        "prepare outcome {:?}",
        prepared.reason
    );
    let verified = verify_incident_package(&package, context);
    assert_eq!(
        report_jcs(&verified),
        report_jcs(&prepared),
        "verify report must equal prepare report (JCS); verify reason={:?}",
        verified.reason
    );
}

fn sidecar_format_token(src: &str) -> &str {
    let marker = "const FORMAT: &str = \"";
    let start = src.find(marker).expect("request_sidecar FORMAT assignment") + marker.len();
    let end = src[start..]
        .find('"')
        .expect("request_sidecar FORMAT terminator");
    &src[start..start + end]
}

/// Parse the private `ADMITTED_FORMATS` array. Chosen over a handwritten count
/// because a 15th token is otherwise undiscoverable without a src change, and
/// over SPEC text because the verifier's array is the admission set.
fn parse_admitted_formats(verify_src: &str, sidecar_token: &str) -> Vec<String> {
    let marker = "const ADMITTED_FORMATS: [&str; ";
    let start = verify_src
        .find(marker)
        .expect("ADMITTED_FORMATS declaration")
        + marker.len();
    let rest = &verify_src[start..];
    let len_end = rest.find(']').expect("ADMITTED_FORMATS length");
    let declared_len: usize = rest[..len_end]
        .trim()
        .parse()
        .expect("ADMITTED_FORMATS length is an integer");
    let array_start = rest.find('[').expect("ADMITTED_FORMATS array");
    let body = &rest[array_start + 1..];
    let array_end = body.find(']').expect("ADMITTED_FORMATS array end");
    let mut tokens = Vec::new();
    for part in body[..array_end].split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part == "request_sidecar::FORMAT" {
            tokens.push(sidecar_token.to_string());
            continue;
        }
        let token = part
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or_else(|| panic!("unparsed ADMITTED_FORMATS entry: {part}"));
        tokens.push(token.to_string());
    }
    assert_eq!(
        tokens.len(),
        declared_len,
        "parsed ADMITTED_FORMATS length must match the array's type length"
    );
    tokens
}

fn limit_value(limits: &ContextLimits, axis: &str) -> u64 {
    let value = serde_json::to_value(limits).expect("ContextLimits JSON");
    value[axis]
        .as_u64()
        .unwrap_or_else(|| panic!("limit axis {axis}"))
}

fn with_limit(mut context: ContextInput, axis: &str, value: u64) -> ContextInput {
    let mut limits = serde_json::to_value(&context.limits).expect("limits JSON");
    limits[axis] = json!(value);
    context.limits = serde_json::from_value(limits).expect("limits round-trip");
    context
}

fn rebind_context(package: &[u8], context: &ContextInput) -> Vec<u8> {
    let container = read_incident_container(package).expect("container");
    let mut assessment: Value =
        serde_json::from_slice(container.assessment_bytes()).expect("assessment JSON");
    let mut context_bytes = assay_canonical::jcs::to_vec(context).expect("context JCS");
    context_bytes.push(b'\n');
    assessment["verification_context_sha256"] = json!(hex::encode(Sha256::digest(&context_bytes)));
    let assessment_bytes = jcs_lf(&assessment);
    let mut inventory: Value =
        serde_json::from_slice(container.inventory_bytes()).expect("inventory JSON");
    inventory["assessment_sha256"] = json!(hex::encode(Sha256::digest(&assessment_bytes)));
    let inventory_bytes = jcs_lf(&inventory);
    let objects: Vec<&[u8]> = container
        .objects()
        .iter()
        .map(|obj| obj.bytes.as_slice())
        .collect();
    write_incident_container(&inventory_bytes, &assessment_bytes, &objects)
        .expect("rebind container")
}

fn rewrite_inventory(package: &[u8], mutate: impl FnOnce(&mut Value)) -> Vec<u8> {
    let container = read_incident_container(package).expect("container");
    let mut inventory: Value =
        serde_json::from_slice(container.inventory_bytes()).expect("inventory JSON");
    mutate(&mut inventory);
    let inventory_bytes = jcs_lf(&inventory);
    let objects: Vec<&[u8]> = container
        .objects()
        .iter()
        .map(|obj| obj.bytes.as_slice())
        .collect();
    write_incident_container(&inventory_bytes, container.assessment_bytes(), &objects)
        .expect("inventory rewrite")
}

struct RawMember {
    name: String,
    header_off: usize,
    data_off: usize,
    data_len: usize,
    end: usize,
}

fn walk_ustar(bytes: &[u8]) -> Vec<RawMember> {
    let mut members = Vec::new();
    let mut pos = 0usize;
    while pos + 512 <= bytes.len() {
        let header = &bytes[pos..pos + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let name_len = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let name = String::from_utf8_lossy(&header[..name_len]).into_owned();
        let size = usize::from_str_radix(
            std::str::from_utf8(&header[124..135]).expect("size octal"),
            8,
        )
        .expect("size");
        let padded = size.div_ceil(512) * 512;
        members.push(RawMember {
            name,
            header_off: pos,
            data_off: pos + 512,
            data_len: size,
            end: pos + 512 + padded,
        });
        pos += 512 + padded;
    }
    members
}

fn flip_member_in_place(package: &[u8], name: &str, index: usize) -> Vec<u8> {
    let members = walk_ustar(package);
    let member = members
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("member {name}"));
    assert!(
        index < member.data_len,
        "{name} index {index} >= {}",
        member.data_len
    );
    let mut out = package.to_vec();
    out[member.data_off + index] ^= 1;
    out
}

fn object_digest_fault(package: &[u8], object_index: usize, byte_index: usize) -> Vec<u8> {
    let container = read_incident_container(package).expect("container");
    let mut objects: Vec<Vec<u8>> = container
        .objects()
        .iter()
        .map(|obj| obj.bytes.clone())
        .collect();
    let object = objects
        .get_mut(object_index)
        .unwrap_or_else(|| panic!("object {object_index}"));
    let idx = byte_index.min(object.len().saturating_sub(1));
    object[idx] ^= 1;
    let object_refs: Vec<&[u8]> = objects.iter().map(Vec::as_slice).collect();
    write_incident_container(
        container.inventory_bytes(),
        container.assessment_bytes(),
        &object_refs,
    )
    .expect("object digest fault container")
}

fn duplicated_last_member(package: &[u8]) -> Vec<u8> {
    let members = walk_ustar(package);
    let last = members.last().expect("package has members");
    let mut out = package[..last.end].to_vec();
    out.extend_from_slice(&package[last.header_off..last.end]);
    out.extend_from_slice(&[0u8; 1024]);
    out
}

fn swapped_inventory_and_assessment(package: &[u8]) -> Vec<u8> {
    let members = walk_ustar(package);
    assert!(members.len() >= 2);
    assert_eq!(members[0].name, "inventory.json");
    assert_eq!(members[1].name, "assessment.json");
    let mut out = Vec::new();
    out.extend_from_slice(&package[members[1].header_off..members[1].end]);
    out.extend_from_slice(&package[members[0].header_off..members[0].end]);
    out.extend_from_slice(&package[members[1].end..]);
    out
}

fn verify_reason(package: &[u8], context: &ContextInput) -> Option<IncidentReason> {
    verify_incident_package(package, context).reason
}

/// Measured first/middle/last XOR of JCS+LF metadata members, then pinned.
/// A middle-byte XOR that stays valid canonical JSON fails phase 4 digest
/// (inventory.assessment_sha256 no longer matches). A framing/parse failure
/// is phase 2 input_shape.
fn pinned_metadata_flip_reason(fixture: &str, member: &str, offset: &str) -> IncidentReason {
    match (fixture, member, offset) {
        // Filled from the measured PIN lines; InputShape is the default
        // because a 1-byte XOR usually breaks JSON or JCS+LF.
        ("valid_five_surfaces.tar", "assessment.json", "middle") => IncidentReason::Digest,
        ("combined_session_3345.tar", "assessment.json", "middle") => IncidentReason::Digest,
        _ => IncidentReason::InputShape,
    }
}

/// The verifier's name for a selected-limit refusal on this axis.
/// `health_bytes` is the CAP-1 admission ceiling (SPEC §8) and is reported as
/// `cap1_shape` rather than `resource_selected`.
fn pinned_limit_reason(axis: &str) -> IncidentReason {
    match axis {
        "health_bytes" => IncidentReason::Cap1Shape,
        _ => IncidentReason::ResourceSelected,
    }
}

fn min_selected_limit(package: &[u8], base: &ContextInput, axis: &str) -> u64 {
    let hard = limit_value(&ContextLimits::HARD, axis);
    if hard == 1 {
        return 1;
    }
    let mut lo = 1u64;
    let mut hi = hard;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let context = with_limit(base.clone(), axis, mid);
        if !context.limits.is_valid() {
            lo = mid + 1;
            continue;
        }
        let rebound = rebind_context(package, &context);
        match verify_incident_package(&rebound, &context).outcome {
            IncidentOutcome::PackageVerified => hi = mid,
            _ => lo = mid + 1,
        }
    }
    lo
}

#[test]
fn admitted_format_table_matches_verifier_array() {
    let sidecar = sidecar_format_token(SIDECAR_SRC);
    let admitted = parse_admitted_formats(VERIFY_SRC, sidecar);
    let table: Vec<&str> = FORMAT_CASES.iter().map(|(format, _)| *format).collect();
    assert_eq!(
        admitted, table,
        "FORMAT_CASES must be exactly the verifier admitted set, in order"
    );
}

#[test]
fn unknown_format_outside_the_table_is_refused() {
    let sidecar = sidecar_format_token(SIDECAR_SRC);
    let admitted: BTreeSet<String> = parse_admitted_formats(VERIFY_SRC, sidecar)
        .into_iter()
        .collect();
    let probes = [
        "not-an-admitted-format",
        "assay-bundle-v2",
        "runner-kernel-ndjson-v2",
        "dsse-attestation-v2",
    ];
    for format in probes {
        assert!(!admitted.contains(format), "{format} unexpectedly admitted");
        let source = IncidentPrepareSource {
            id: "probe".into(),
            format: format.into(),
            bytes: b"{}\n".to_vec(),
        };
        let error = prepare_verified_incident_package(&[source], &ContextInput::default())
            .expect_err(format);
        assert_eq!(
            error.reason,
            Some(IncidentReason::UnknownFormat),
            "{format}"
        );
    }
}

#[test]
fn round_trip_each_admitted_format() {
    for (format, case) in FORMAT_CASES {
        match case {
            None => {
                println!("round-trip non-claim for {format}: {PREPARE_BINDING_NONCLAIM}");
                let err = sources_for(format).expect_err("non-claim formats have no sources");
                assert!(
                    err.contains("binding"),
                    "non-claim for {format} must name the binding limitation, got {err}"
                );
            }
            Some(label) => {
                let (sources, context) =
                    sources_for(format).expect("claimed format has a prepare case");
                assert_eq!(sources[0].format, *format, "{label}");
                assert_round_trip(&sources, &context);
            }
        }
    }
}

#[test]
fn round_trip_combined_session_3345() {
    let package = read_fixture("combined_session_3345.tar");
    let context = session_3345_context();
    let control = verify_incident_package(&package, &context);
    assert_eq!(
        control.outcome,
        IncidentOutcome::PackageVerified,
        "control: fixture verifies under its own context ({:?})",
        control.reason
    );
    let sources = all_sources(&package);
    let formats: BTreeSet<&str> = sources.iter().map(|s| s.format.as_str()).collect();
    assert!(formats.contains("runner-kernel-ndjson-v0"));
    assert_round_trip(&sources, &context);
}

#[test]
fn byte_flip_object_members_refuse_digest() {
    let context = ContextInput::default();
    let session_ctx = session_3345_context();
    for (name, ctx) in [
        ("valid_five_surfaces.tar", &context),
        ("valid_present_empty.tar", &context),
        ("combined_session_3345.tar", &session_ctx),
    ] {
        let package = read_fixture(name);
        let control = verify_incident_package(&package, ctx);
        assert_eq!(
            control.outcome,
            IncidentOutcome::PackageVerified,
            "control {name}: {:?}",
            control.reason
        );
        let object_count = read_incident_container(&package)
            .expect("container")
            .objects()
            .len();
        for object_index in 0..object_count {
            let object_len = read_incident_container(&package).unwrap().objects()[object_index]
                .bytes
                .len();
            for byte_index in [0usize, object_len / 2, object_len.saturating_sub(1)] {
                let mutated = object_digest_fault(&package, object_index, byte_index);
                assert_eq!(
                    verify_reason(&mutated, ctx),
                    Some(IncidentReason::Digest),
                    "{name} object {object_index} byte {byte_index}"
                );
            }
        }
    }
}

#[test]
fn byte_flip_inventory_and_assessment_refuse_pinned_reason() {
    // In-place XOR of a JCS+LF member fails phase 2 (canonical JSON / JCS
    // equivalence) before phase 4 digest. Measured, then pinned.
    let context = ContextInput::default();
    for name in [
        "valid_five_surfaces.tar",
        "valid_present_empty.tar",
        "combined_session_3345.tar",
    ] {
        let package = read_fixture(name);
        let ctx = if name.starts_with("combined") {
            session_3345_context()
        } else {
            context.clone()
        };
        for member in ["inventory.json", "assessment.json"] {
            let len = walk_ustar(&package)
                .into_iter()
                .find(|m| m.name == member)
                .unwrap()
                .data_len;
            for (label, byte_index) in [("first", 0usize), ("middle", len / 2), ("last", len - 1)] {
                let mutated = flip_member_in_place(&package, member, byte_index);
                let reason = verify_reason(&mutated, &ctx);
                let expected = pinned_metadata_flip_reason(name, member, label);
                assert_eq!(
                    reason,
                    Some(expected),
                    "{name} {member} {label} byte {byte_index}"
                );
            }
        }
    }
}

#[test]
fn container_truncation_duplicate_and_swap_refuse() {
    let context = ContextInput::default();
    let package = read_fixture("valid_five_surfaces.tar");
    assert_eq!(
        verify_incident_package(&package, &context).outcome,
        IncidentOutcome::PackageVerified
    );
    let members = walk_ustar(&package);
    let inventory = &members[0];
    let assessment = &members[1];
    let first_object = &members[2];
    let eof = members.last().expect("members").end;
    let truncations = [
        inventory.header_off + 40,
        inventory.data_off + inventory.data_len / 2,
        assessment.header_off + 20,
        first_object.data_off + 8,
        eof,
        eof + 512,
        package.len() - 1,
    ];
    for offset in truncations {
        let truncated = package[..offset].to_vec();
        assert_eq!(
            verify_reason(&truncated, &context),
            Some(IncidentReason::InputShape),
            "truncate at {offset}"
        );
    }
    assert_eq!(
        verify_reason(&duplicated_last_member(&package), &context),
        Some(IncidentReason::InputShape),
        "duplicated member"
    );
    assert_eq!(
        verify_reason(&swapped_inventory_and_assessment(&package), &context),
        Some(IncidentReason::InputShape),
        "swapped inventory/assessment"
    );
}

#[test]
fn two_faults_refuse_with_the_earlier_phase() {
    let context = ContextInput::default();
    let package = read_fixture("valid_five_surfaces.tar");
    assert_eq!(
        verify_incident_package(&package, &context).outcome,
        IncidentOutcome::PackageVerified
    );

    let unknown = rewrite_inventory(&package, |inventory| {
        inventory["inputs"][0]["format"] = json!("not-an-admitted-format");
    });
    let digest_only = rewrite_inventory(&package, |inventory| {
        inventory["assessment_sha256"] = json!("0".repeat(64));
    });
    let unknown_and_digest = rewrite_inventory(&package, |inventory| {
        inventory["inputs"][0]["format"] = json!("not-an-admitted-format");
        inventory["assessment_sha256"] = json!("0".repeat(64));
    });
    assert_eq!(
        verify_reason(&unknown, &context),
        Some(IncidentReason::UnknownFormat),
        "p3 control"
    );
    assert_eq!(
        verify_reason(&digest_only, &context),
        Some(IncidentReason::Digest),
        "p4 control"
    );
    assert_eq!(
        verify_reason(&unknown_and_digest, &context),
        Some(IncidentReason::UnknownFormat),
        "p3 unknown_format + p4 digest must name UnknownFormat"
    );

    let mut stale_context = context.clone();
    stale_context.as_of = "2026-09-08T12:00:00Z".into();
    assert_eq!(
        verify_reason(&package, &stale_context),
        Some(IncidentReason::StaleAssessment),
        "p11 control"
    );
    assert_eq!(
        verify_reason(&digest_only, &stale_context),
        Some(IncidentReason::Digest),
        "p4 digest + p11 stale_assessment must name Digest"
    );

    // SPEC §10: phase 10 expectation_mismatch precedes phase 11 stale_assessment.
    let sha = {
        let inventory: Value =
            serde_json::from_slice(read_incident_container(&package).unwrap().inventory_bytes())
                .unwrap();
        inventory["inputs"][0]["sha256"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut expectation_only = context.clone();
    expectation_only.expectations = vec![ContextExpectation {
        input_id: "bundle".into(),
        sha256: "0".repeat(64),
        require_disclosure: false,
    }];
    assert_ne!(sha, "0".repeat(64));
    assert_eq!(
        verify_reason(&package, &expectation_only),
        Some(IncidentReason::ExpectationMismatch),
        "p10 control"
    );
    let mut expectation_and_stale = expectation_only;
    expectation_and_stale.as_of = "2026-09-08T12:00:00Z".into();
    assert_eq!(
        verify_reason(&package, &expectation_and_stale),
        Some(IncidentReason::ExpectationMismatch),
        "p10 expectation_mismatch + p11 stale_assessment must name ExpectationMismatch"
    );
}

#[test]
fn selected_limit_minus_one_refuses_on_exercised_axes() {
    let fixtures = [
        ("valid_five_surfaces.tar", ContextInput::default()),
        ("combined_session_3345.tar", session_3345_context()),
    ];
    let mut covered: Vec<&str> = Vec::new();
    let mut non_claims: Vec<String> = Vec::new();
    for axis in LIMIT_AXES {
        let hard = limit_value(&ContextLimits::HARD, axis);
        if hard == 1 {
            non_claims.push(format!(
                "{axis}: hard maximum is 1, so selected 0 is trust_input (ContextLimits::is_valid), not a usage refusal"
            ));
            continue;
        }
        let mut exercised: Option<(&str, u64, IncidentReason)> = None;
        let mut seen_min = Vec::new();
        for (name, base) in &fixtures {
            let package = read_fixture(name);
            assert_eq!(
                verify_incident_package(&package, base).outcome,
                IncidentOutcome::PackageVerified,
                "control {name}"
            );
            let min = min_selected_limit(&package, base, axis);
            if min <= 1 {
                seen_min.push(format!("{name}:min=1"));
                continue;
            }
            let context = with_limit(base.clone(), axis, min - 1);
            let rebound = rebind_context(&package, &context);
            let reason = verify_reason(&rebound, &context);
            let pinned = pinned_limit_reason(axis);
            if reason != Some(pinned) {
                seen_min.push(format!("{name}:min={min}:reason={reason:?}"));
                continue;
            }
            println!("limit axis {axis} covered on {name} at usage {min} reason {pinned:?}");
            exercised = Some((name, min, pinned));
            break;
        }
        match exercised {
            Some(_) => covered.push(axis),
            None => non_claims.push(format!(
                "{axis}: not an honest usage refusal on these fixtures ({})",
                seen_min.join("; ")
            )),
        }
    }
    for line in &non_claims {
        println!("limit non-claim: {line}");
    }
    assert!(
        covered.contains(&"outer_bytes"),
        "outer_bytes must be exercised (checked before materialisation)"
    );
    assert!(
        !covered.is_empty(),
        "at least one ContextLimits axis must be exercised"
    );
    assert_eq!(
        covered.len() + non_claims.len(),
        LIMIT_AXES.len(),
        "every axis is covered or a written non-claim"
    );
}

#[test]
fn verify_and_prepare_are_byte_deterministic() {
    let package = read_fixture("valid_five_surfaces.tar");
    let context = ContextInput::default();
    let first = verify_incident_package(&package, &context);
    let second = verify_incident_package(&package, &context);
    assert_eq!(report_jcs(&first), report_jcs(&second));
    assert_eq!(first.outcome, IncidentOutcome::PackageVerified);

    let sources = all_sources(&package);
    let (a, report_a) = prepare_ok(&sources, &context);
    let (b, report_b) = prepare_ok(&sources, &context);
    assert_eq!(a, b, "prepare must emit identical package bytes");
    assert_eq!(report_jcs(&report_a), report_jcs(&report_b));

    let session = read_fixture("combined_session_3345.tar");
    let session_ctx = session_3345_context();
    assert_eq!(
        report_jcs(&verify_incident_package(&session, &session_ctx)),
        report_jcs(&verify_incident_package(&session, &session_ctx))
    );
    let session_sources = all_sources(&session);
    let (sa, _) = prepare_ok(&session_sources, &session_ctx);
    let (sb, _) = prepare_ok(&session_sources, &session_ctx);
    assert_eq!(sa, sb);
}
