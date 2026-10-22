use assay_evidence::incident_package::verify::build_verified_incident_package;
use assay_evidence::incident_package::{
    read_incident_container, verify_incident_package, write_incident_container,
    ContextApplicability, ContextInput, ContextLimits, ContextWindow, IncidentExpectation,
    IncidentOutcome, IncidentReason, IncidentVerifyReport, PackageRef, INCIDENT_VERIFY_SCHEMA_V1,
    NON_CLAIMS_DEFAULT,
};
use assay_evidence::{verify_cap1_document, Cap1AdmissionLimits};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const KERNEL_FORMAT: &str = "runner-kernel-ndjson-v0";
const KERNEL_SCHEMA: &str = "assay.runner.kernel_event.v0";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn jcs_line_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = assay_canonical::jcs::to_vec(value).expect("JCS canonicalization");
    bytes.push(b'\n');
    bytes
}

fn context_sha256_hex(context: &ContextInput) -> String {
    let mut bytes = assay_canonical::jcs::to_vec(context).expect("ContextInput JCS");
    bytes.push(b'\n');
    sha256_hex(&bytes)
}

fn unit_id_hex(input_id: &str, sha256: &str, locator: &str) -> String {
    let bytes = assay_canonical::jcs::to_vec(&json!([input_id, sha256, locator]))
        .expect("unit ID JCS tuple");
    sha256_hex(&bytes)
}

fn default_inventory_context() -> Value {
    let entry = json!({
        "value": null,
        "reason": "not_available",
        "refs": []
    });
    json!({
        "original_digests": entry.clone(),
        "chain_gaps": entry.clone(),
        "custody": entry.clone(),
        "producer_invocation": entry.clone(),
        "clock": entry.clone(),
        "actor": entry.clone(),
        "correlation": entry.clone(),
        "policy_activation": entry.clone(),
        "recoverability": entry.clone(),
        "resource_effects": entry.clone(),
        "context_provenance": entry.clone(),
        "denominator": entry.clone(),
        "missing_evidence": entry.clone(),
        "withholding_derivatives": entry.clone(),
        "retention": entry.clone(),
        "offline_verification": entry.clone(),
        "signature_basis": entry.clone(),
        "schema_versions": entry.clone(),
        "windows": entry
    })
}

fn cap1_stratum(id: &str, eligible: u64, examined: u64) -> Value {
    json!({
        "id": id,
        "population": "retained supplied records",
        "basis": {
            "kind": "enumeration",
            "enumeration_method": "incident-v1-digest-locator-enumeration"
        },
        "eligible": eligible,
        "examined": examined,
        "unexamined": [],
        "supports": ["positive_presence", "bounded_negative", "exhaustive_set"]
    })
}

fn cap1_document(strata: Vec<Value>) -> Value {
    let doc = json!({
        "profile": "cap/1",
        "subject": {
            "kind": "collection",
            "ref": "incident-selected-inputs"
        },
        "integrity": {
            "complete": true,
            "statement": "accounting of selected supplied records",
            "capped_to": null
        },
        "strata": strata
    });
    let bytes = serde_json::to_vec(&doc).expect("CAP-1 bytes");
    verify_cap1_document(&bytes, &Cap1AdmissionLimits::default())
        .expect("helper CAP-1 document must satisfy normative CAP-1 schema and rules");
    doc
}

fn examined_unit(
    input_id: &str,
    sha256: &str,
    locator: &str,
    surface: &str,
    value: Value,
) -> Value {
    json!({
        "id": unit_id_hex(input_id, sha256, locator),
        "source": {
            "input_id": input_id,
            "sha256": sha256,
            "locator": locator
        },
        "surface": surface,
        "disposition": "examined",
        "result": {
            "mode": "inline",
            "value": {
                "operation": KERNEL_FORMAT,
                "source": {
                    "input_id": input_id,
                    "sha256": sha256,
                    "locator": locator
                },
                "value": value
            }
        },
        "withheld_digest": null
    })
}

fn sort_units_by_id(units: &mut [Value]) {
    units.sort_by(|left, right| {
        left["id"]
            .as_str()
            .expect("unit id")
            .cmp(right["id"].as_str().expect("unit id"))
    });
}

fn surface_row(
    name: &str,
    source_inputs: &[&str],
    observation: &str,
    basis_refs: Value,
    window: Value,
) -> Value {
    json!({
        "name": name,
        "source_inputs": source_inputs,
        "observation": observation,
        "basis_refs": basis_refs,
        "window": window
    })
}

fn build_package(
    inventory_inputs: Value,
    assessment: &Value,
    objects: &[&[u8]],
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let assessment_bytes = jcs_line_bytes(assessment);
    let inventory = json!({
        "schema": "assay.incident.inventory.v1",
        "assessment_sha256": sha256_hex(&assessment_bytes),
        "inputs": inventory_inputs,
        "context": default_inventory_context(),
        "non_claims": NON_CLAIMS_DEFAULT
    });
    let inventory_bytes = jcs_line_bytes(&inventory);
    let package_bytes = write_incident_container(&inventory_bytes, &assessment_bytes, objects)
        .expect("canonical container framing");
    (package_bytes, inventory_bytes, assessment_bytes)
}

fn single_line_kernel_package(line: &str, declared_surfaces: &[&str]) -> Vec<u8> {
    single_line_kernel_package_in_format(line, declared_surfaces, KERNEL_FORMAT)
}

fn single_line_kernel_package_in_format(
    line: &str,
    declared_surfaces: &[&str],
    format: &str,
) -> Vec<u8> {
    let raw_bytes = format!("{line}\n").into_bytes();
    let sha = sha256_hex(&raw_bytes);
    let locator = format!("line:0:0:{}", line.len());
    let parsed_val: Value = serde_json::from_str(line).unwrap_or_else(|_| json!({}));
    let unit_surface = declared_surfaces.first().copied().unwrap_or("filesystem");
    let mut units = vec![examined_unit(
        "kernel",
        &sha,
        &locator,
        unit_surface,
        parsed_val,
    )];
    sort_units_by_id(&mut units);

    let fs_sources: &[&str] = if declared_surfaces.contains(&"filesystem") {
        &["kernel"]
    } else {
        &[]
    };
    let net_sources: &[&str] = if declared_surfaces.contains(&"network") {
        &["kernel"]
    } else {
        &[]
    };
    let proc_sources: &[&str] = if declared_surfaces.contains(&"process") {
        &["kernel"]
    } else {
        &[]
    };

    let mut strata = Vec::new();
    for &surface in &["filesystem", "network", "process"] {
        if declared_surfaces.contains(&surface) {
            let count = u64::from(surface == unit_surface);
            strata.push(cap1_stratum(surface, count, count));
        }
    }
    let cap1 = if strata.is_empty() {
        Value::Null
    } else {
        cap1_document(strata)
    };

    let ctx = ContextInput::default();
    let assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", fs_sources, "unknown", json!([]), Value::Null),
            surface_row("network", net_sources, "unknown", json!([]), Value::Null),
            surface_row("process", proc_sources, "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1,
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let inputs = json!([{
        "id": "kernel",
        "format": format,
        "sha256": sha,
        "bytes": raw_bytes.len() as u64,
        "surfaces": declared_surfaces,
        "binding": null
    }]);
    let (pkg, _, _) = build_package(inputs, &assessment, &[&raw_bytes]);
    pkg
}

fn raw_kernel_bytes_package(
    raw_bytes: &[u8],
    declared_surfaces: &[&str],
    ctx: &ContextInput,
) -> Vec<u8> {
    let sha = sha256_hex(raw_bytes);
    let proc_sources: &[&str] = if declared_surfaces.contains(&"process") {
        &["kernel"]
    } else {
        &[]
    };
    let assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(ctx),
        "units": [],
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", proc_sources, "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 0, 0)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let inputs = json!([{
        "id": "kernel",
        "format": KERNEL_FORMAT,
        "sha256": sha,
        "bytes": raw_bytes.len() as u64,
        "surfaces": declared_surfaces,
        "binding": null
    }]);
    let (pkg, _, _) = build_package(inputs, &assessment, &[raw_bytes]);
    pkg
}

#[test]
fn kernel_three_surfaces_duplicate_occurrences_and_byte_identical_export_verify() {
    let ctx = ContextInput::default();
    let expected_ctx_sha = context_sha256_hex(&ctx);

    // Positive control on existing admitted format ("proxy-decision-ndjson") proving that
    // build_package, default_inventory_context, cap1_document, cap1_stratum, and surface_row
    // satisfy all 11 verification phases on baseline.
    let control_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": expected_ctx_sha,
        "units": [],
        "surfaces": [
            surface_row("tool_call", &["proxy-control"], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("tool_call", 0, 0)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let control_inputs = json!([{
        "id": "proxy-control",
        "format": "proxy-decision-ndjson",
        "sha256": sha256_hex(b""),
        "bytes": 0,
        "surfaces": ["tool_call"],
        "binding": null
    }]);
    let (control_pkg, _, _) = build_package(control_inputs, &control_assessment, &[b""]);
    let control_report = verify_incident_package(&control_pkg, &ctx);
    assert_eq!(control_report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(control_report.reason, None);

    let line0 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":0,"pid":101,"event_type":1,"kind":"openat","value":"/tmp/out.txt","flags":577,"mode":420,"return_value":3,"access_mode":"write","operation_flags":["create","truncate"],"status":"success"}"#;
    let line1 = line0;
    let line2 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":1,"pid":101,"event_type":20,"kind":"connect_blocked","value":"198.51.100.7:8443","cgroup_id":4242,"network_destination":"198.51.100.7","network_port":8443,"rule_id":9}"#;
    let line3 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":2,"pid":102,"event_type":4,"kind":"exec","value":"/usr/bin/curl"}"#;

    let raw_ndjson = format!("{line0}\n{line1}\n{line2}\n{line3}\n");
    let raw_bytes = raw_ndjson.as_bytes();
    let kernel_sha = sha256_hex(raw_bytes);

    let off0 = 0usize;
    let off1 = line0.len() + 1;
    let off2 = off1 + line1.len() + 1;
    let off3 = off2 + line2.len() + 1;

    let loc0 = format!("line:0:{off0}:{}", line0.len());
    let loc1 = format!("line:1:{off1}:{}", line1.len());
    let loc2 = format!("line:2:{off2}:{}", line2.len());
    let loc3 = format!("line:3:{off3}:{}", line3.len());
    assert_ne!(
        loc0, loc1,
        "duplicate raw lines must have distinct locators"
    );

    let val0: Value = serde_json::from_str(line0).expect("line0 JSON");
    let val1: Value = serde_json::from_str(line1).expect("line1 JSON");
    let val2: Value = serde_json::from_str(line2).expect("line2 JSON");
    let val3: Value = serde_json::from_str(line3).expect("line3 JSON");

    let mut units = vec![
        examined_unit("kernel-main", &kernel_sha, &loc0, "filesystem", val0),
        examined_unit("kernel-main", &kernel_sha, &loc1, "filesystem", val1),
        examined_unit("kernel-main", &kernel_sha, &loc2, "network", val2),
        examined_unit("kernel-main", &kernel_sha, &loc3, "process", val3),
    ];
    sort_units_by_id(&mut units);

    let assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": expected_ctx_sha,
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel-main"], "unknown", json!([]), Value::Null),
            surface_row("network", &["kernel-main"], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel-main"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![
            cap1_stratum("filesystem", 2, 2),
            cap1_stratum("network", 1, 1),
            cap1_stratum("process", 1, 1)
        ]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let inputs = json!([{
        "id": "kernel-main",
        "format": KERNEL_FORMAT,
        "sha256": kernel_sha,
        "bytes": raw_bytes.len() as u64,
        "surfaces": ["filesystem", "network", "process"],
        "binding": null
    }]);
    let (package_bytes, inventory_bytes, assessment_bytes) =
        build_package(inputs, &assessment, &[raw_bytes]);

    let expected_report = IncidentVerifyReport {
        schema: INCIDENT_VERIFY_SCHEMA_V1.to_string(),
        outcome: IncidentOutcome::PackageVerified,
        reason: None,
        artifact_sha256: Some(sha256_hex(&package_bytes)),
        inventory_sha256: Some(sha256_hex(&inventory_bytes)),
        assessment_sha256: Some(sha256_hex(&assessment_bytes)),
        expectation: IncidentExpectation::NotRequested,
        attestations: Vec::new(),
        verification_context: Some(serde_json::to_value(&ctx).expect("ContextInput value")),
        verification_context_sha256: Some(expected_ctx_sha),
        resolved_results: Vec::new(),
        counts: Some(json!({
            "inputs": 1,
            "objects": 1,
            "units": 4,
            "examined": 4,
            "withheld": 0,
            "resolved_result_count": 0
        })),
        non_claims: NON_CLAIMS_DEFAULT
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };

    let actual_report = verify_incident_package(&package_bytes, &ctx);
    assert_eq!(actual_report, expected_report);

    let (exported_bytes, export_report) =
        build_verified_incident_package(&inventory_bytes, &assessment_bytes, &[raw_bytes], &ctx)
            .expect("verified kernel incident package export");
    assert_eq!(
        exported_bytes, package_bytes,
        "export must be byte-identical"
    );
    assert_eq!(export_report, expected_report);

    let container = read_incident_container(&exported_bytes).expect("read exported container");
    assert_eq!(container.objects.len(), 1);
    assert_eq!(container.objects[0].bytes, raw_bytes);
}

#[test]
fn kernel_all_emitted_variants_optional_fields_and_present_empty_verify() {
    let l0 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":10,"pid":200,"event_type":1,"kind":"openat","value":"/etc/hosts","flags":0,"mode":0,"resolve":4,"return_value":-2,"access_mode":"read","status":"error"}"#;
    let l1 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":11,"pid":200,"event_type":10,"kind":"file_blocked","value":"/etc/shadow"}"#;
    let l2 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":12,"pid":200,"event_type":2,"kind":"connect","value":null}"#;
    let l3 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":13,"pid":200,"event_type":6,"kind":"sendto","value":"10.0.0.1:53"}"#;
    let l4 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":14,"pid":200,"event_type":7,"kind":"sendmsg","value":"2001:db8::1:443"}"#;
    let l5 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run-02","seq":15,"pid":200,"event_type":20,"kind":"connect_blocked","value":null}"#;

    // Mix CRLF and LF with a final valid unterminated line to verify exact byte-start and byte-length.
    let raw_ndjson = format!("{l0}\r\n{l1}\n{l2}\r\n{l3}\n{l4}\r\n{l5}");
    let raw_bytes = raw_ndjson.as_bytes();
    let sha_variants = sha256_hex(raw_bytes);
    let empty_bytes: &[u8] = b"";
    let sha_empty = sha256_hex(empty_bytes);

    let off0 = 0usize;
    let off1 = off0 + l0.len() + 2;
    let off2 = off1 + l1.len() + 1;
    let off3 = off2 + l2.len() + 2;
    let off4 = off3 + l3.len() + 1;
    let off5 = off4 + l4.len() + 2;

    let mut units = vec![
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:0:{off0}:{}", l0.len()),
            "filesystem",
            serde_json::from_str(l0).unwrap(),
        ),
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:1:{off1}:{}", l1.len()),
            "filesystem",
            serde_json::from_str(l1).unwrap(),
        ),
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:2:{off2}:{}", l2.len()),
            "network",
            serde_json::from_str(l2).unwrap(),
        ),
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:3:{off3}:{}", l3.len()),
            "network",
            serde_json::from_str(l3).unwrap(),
        ),
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:4:{off4}:{}", l4.len()),
            "network",
            serde_json::from_str(l4).unwrap(),
        ),
        examined_unit(
            "kernel-variants",
            &sha_variants,
            &format!("line:5:{off5}:{}", l5.len()),
            "network",
            serde_json::from_str(l5).unwrap(),
        ),
    ];
    sort_units_by_id(&mut units);

    let ctx = ContextInput::default();
    let assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel-variants"], "unknown", json!([]), Value::Null),
            surface_row("network", &["kernel-variants"], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel-empty"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![
            cap1_stratum("filesystem", 2, 2),
            cap1_stratum("network", 4, 4),
            cap1_stratum("process", 0, 0)
        ]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let inputs = json!([
        {
            "id": "kernel-empty",
            "format": KERNEL_FORMAT,
            "sha256": sha_empty,
            "bytes": 0,
            "surfaces": ["process"],
            "binding": null
        },
        {
            "id": "kernel-variants",
            "format": KERNEL_FORMAT,
            "sha256": sha_variants,
            "bytes": raw_bytes.len() as u64,
            "surfaces": ["filesystem", "network"],
            "binding": null
        }
    ]);
    let (package_bytes, _, _) = build_package(inputs, &assessment, &[empty_bytes, raw_bytes]);
    let report = verify_incident_package(&package_bytes, &ctx);
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(report.reason, None);
    assert_eq!(
        report.counts,
        Some(json!({
            "inputs": 2,
            "objects": 2,
            "units": 6,
            "examined": 6,
            "withheld": 0,
            "resolved_result_count": 0
        }))
    );
}

#[test]
fn kernel_surface_observation_is_unknown_without_context_and_at_most_limited_with_complete_health()
{
    let line = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_obs","seq":0,"pid":77,"event_type":1,"kind":"openat","value":"/tmp/a","flags":0,"mode":420,"return_value":3,"access_mode":"read","status":"success"}"#;
    let kernel_bytes = format!("{line}\n").into_bytes();
    let kernel_sha = sha256_hex(&kernel_bytes);
    let locator = format!("line:0:0:{}", line.len());

    let health_doc = json!({
        "schema": "assay.runner.observation_health.v0",
        "run_id": "run_obs",
        "platform": "linux",
        "kernel_layer": "complete",
        "capture_origin": "own",
        "ringbuf_drops": 0,
        "policy_layer": "present",
        "sdk_layer": "absent",
        "cgroup_correlation": "clean",
        "network_protocol_coverage": "connect_and_datagram_peer_observed",
        "network_endpoint_claim_scope": "peer_set",
        "notes": []
    });
    let health_bytes = jcs_line_bytes(&health_doc);
    let health_sha = sha256_hex(&health_bytes);

    let units = vec![examined_unit(
        "kernel",
        &kernel_sha,
        &locator,
        "filesystem",
        serde_json::from_str(line).unwrap(),
    )];
    let inputs = json!([
        {
            "id": "health",
            "format": "assay-health-json",
            "sha256": health_sha,
            "bytes": health_bytes.len() as u64,
            "surfaces": [],
            "binding": null
        },
        {
            "id": "kernel",
            "format": KERNEL_FORMAT,
            "sha256": kernel_sha,
            "bytes": kernel_bytes.len() as u64,
            "surfaces": ["filesystem"],
            "binding": null
        }
    ]);

    // 1. Without applicability context, raw kernel surface observation is pinned to "unknown".
    let default_ctx = ContextInput::default();
    let unknown_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&default_ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("filesystem", 1, 1)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let (unknown_pkg, _, _) = build_package(
        inputs.clone(),
        &unknown_assessment,
        &[&health_bytes, &kernel_bytes],
    );
    let unknown_report = verify_incident_package(&unknown_pkg, &default_ctx);
    assert_eq!(unknown_report.outcome, IncidentOutcome::PackageVerified);

    // 2. Even when operator requests "adequate" with complete ObservationHealth and no full
    // coverage evidence, supplied raw kernel surface is capped at "limited".
    let window = ContextWindow {
        start: "2026-09-08T00:00:00Z".to_string(),
        end: "2026-09-08T00:01:00Z".to_string(),
        clock_basis: "operator-stated".to_string(),
    };
    let health_ref = PackageRef {
        input_id: "health".to_string(),
        sha256: health_sha.clone(),
        locator: "json:".to_string(),
    };
    let adequate_ctx = ContextInput {
        applicability: vec![ContextApplicability {
            surface: "filesystem".to_string(),
            input_refs: vec![health_ref.clone()],
            window: Some(window.clone()),
            coverage: "adequate".to_string(),
            retention: "adequate".to_string(),
            correspondence: "unique".to_string(),
            source_class: "independently_observed".to_string(),
        }],
        ..ContextInput::default()
    };

    let limited_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&adequate_ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row(
                "filesystem",
                &["kernel"],
                "limited",
                json!([health_ref]),
                serde_json::to_value(&window).unwrap()
            ),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("filesystem", 1, 1)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let (limited_pkg, _, _) = build_package(
        inputs.clone(),
        &limited_assessment,
        &[&health_bytes, &kernel_bytes],
    );
    let limited_report = verify_incident_package(&limited_pkg, &adequate_ctx);
    assert_eq!(limited_report.outcome, IncidentOutcome::PackageVerified);

    // 3. Recording "adequate" on the raw kernel surface refuses as StaleAssessment.
    let mut overclaimed_assessment = limited_assessment.clone();
    overclaimed_assessment["surfaces"][1]["observation"] = json!("adequate");
    let (overclaimed_pkg, _, _) = build_package(
        inputs,
        &overclaimed_assessment,
        &[&health_bytes, &kernel_bytes],
    );
    let overclaimed_report = verify_incident_package(&overclaimed_pkg, &adequate_ctx);
    assert_eq!(overclaimed_report.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(
        overclaimed_report.reason,
        Some(IncidentReason::StaleAssessment)
    );

    // 4. Adding an otherwise valid, unreferenced Full filesystem CoverageDescriptor alongside
    // applicability.input_refs referencing only health must NOT lift the raw kernel surface
    // ceiling above "limited": "limited" stays PackageVerified and "adequate" refuses with
    // StaleAssessment.
    let coverage_doc = json!({
        "schema": "assay.runner.coverage_descriptor.v0",
        "dimension": "filesystem",
        "method": "open/openat/openat2 tracepoints",
        "observes": ["path opens through syscall tracepoints"],
        "known_blind_spots": [],
        "completeness": "full"
    });
    let coverage_bytes = jcs_line_bytes(&coverage_doc);
    let coverage_sha = sha256_hex(&coverage_bytes);
    let inputs_with_unreferenced_full_coverage = json!([
        {
            "id": "coverage",
            "format": "assay-coverage-json",
            "sha256": coverage_sha,
            "bytes": coverage_bytes.len() as u64,
            "surfaces": [],
            "binding": null
        },
        {
            "id": "health",
            "format": "assay-health-json",
            "sha256": health_sha,
            "bytes": health_bytes.len() as u64,
            "surfaces": [],
            "binding": null
        },
        {
            "id": "kernel",
            "format": KERNEL_FORMAT,
            "sha256": kernel_sha,
            "bytes": kernel_bytes.len() as u64,
            "surfaces": ["filesystem"],
            "binding": null
        }
    ]);
    let (limited_with_cov_pkg, _, _) = build_package(
        inputs_with_unreferenced_full_coverage.clone(),
        &limited_assessment,
        &[&coverage_bytes, &health_bytes, &kernel_bytes],
    );
    let limited_with_cov_report = verify_incident_package(&limited_with_cov_pkg, &adequate_ctx);

    let (overclaimed_with_cov_pkg, _, _) = build_package(
        inputs_with_unreferenced_full_coverage,
        &overclaimed_assessment,
        &[&coverage_bytes, &health_bytes, &kernel_bytes],
    );
    let overclaimed_with_cov_report =
        verify_incident_package(&overclaimed_with_cov_pkg, &adequate_ctx);
    assert_eq!(
        overclaimed_with_cov_report.outcome,
        IncidentOutcome::PackageRefused
    );
    assert_eq!(
        overclaimed_with_cov_report.reason,
        Some(IncidentReason::StaleAssessment)
    );
    assert_eq!(
        limited_with_cov_report.outcome,
        IncidentOutcome::PackageVerified
    );
    assert_eq!(limited_with_cov_report.reason, None);
}

// Linux open(2) bits: O_EXCL is 0o200 and O_NOCTTY is 0o400. 524481 is
// O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC and 655616 is O_CLOEXEC|O_NOFOLLOW|O_NOCTTY.
const KERNEL_FORMAT_V1: &str = "runner-kernel-ndjson-v1";
const V1_O_EXCL: &str = r#"{"schema":"assay.runner.kernel_event.v1","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":524481,"mode":384,"return_value":3,"access_mode":"write","operation_flags":["create","exclusive"],"status":"success"}"#;
const V1_O_NOCTTY: &str = r#"{"schema":"assay.runner.kernel_event.v1","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":655616,"mode":0,"return_value":3,"access_mode":"read","status":"success"}"#;
const V0_O_EXCL: &str = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":524481,"mode":384,"return_value":3,"access_mode":"write","operation_flags":["create"],"status":"success"}"#;
const V0_O_NOCTTY: &str = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":655616,"mode":0,"return_value":3,"access_mode":"read","operation_flags":["exclusive"],"status":"success"}"#;

#[test]
fn kernel_v1_format_admits_corrected_open_labels() {
    let ctx = ContextInput::default();
    for line in [V1_O_EXCL, V1_O_NOCTTY] {
        let pkg = single_line_kernel_package_in_format(line, &["filesystem"], KERNEL_FORMAT_V1);
        let report = verify_incident_package(&pkg, &ctx);
        assert_eq!(report.outcome, IncidentOutcome::PackageVerified, "{line}");
        assert_eq!(report.reason, None, "{line}");
    }
    let legacy_label = V1_O_EXCL.replace(r#"["create","exclusive"]"#, r#"["create"]"#);
    let pkg =
        single_line_kernel_package_in_format(&legacy_label, &["filesystem"], KERNEL_FORMAT_V1);
    assert_eq!(
        verify_incident_package(&pkg, &ctx).reason,
        Some(IncidentReason::InputShape)
    );
}

#[test]
fn kernel_v0_format_keeps_its_frozen_open_labels_and_formats_do_not_mix() {
    let ctx = ContextInput::default();
    for line in [V0_O_EXCL, V0_O_NOCTTY] {
        let report =
            verify_incident_package(&single_line_kernel_package(line, &["filesystem"]), &ctx);
        assert_eq!(report.outcome, IncidentOutcome::PackageVerified, "{line}");
    }
    let v1_in_v0 = single_line_kernel_package(V1_O_EXCL, &["filesystem"]);
    assert_eq!(
        verify_incident_package(&v1_in_v0, &ctx).reason,
        Some(IncidentReason::UnknownSchema)
    );
    let v0_in_v1 =
        single_line_kernel_package_in_format(V0_O_EXCL, &["filesystem"], KERNEL_FORMAT_V1);
    assert_eq!(
        verify_incident_package(&v0_in_v1, &ctx).reason,
        Some(IncidentReason::UnknownSchema)
    );
}

#[test]
fn kernel_rejects_invalid_schema_and_inconsistent_event_type_kind_or_fields() {
    let ctx = ContextInput::default();

    // Wrong schema string -> UnknownSchema
    let wrong_schema = r#"{"schema":"assay.runner.kernel_event.v1","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#;
    let pkg = single_line_kernel_package(wrong_schema, &["process"]);
    assert_eq!(
        verify_incident_package(&pkg, &ctx).reason,
        Some(IncidentReason::UnknownSchema)
    );

    // Non-string schema, mismatched event_type/kind, unmapped event_type, and invalid variant fields -> InputShape
    let invalid_shape_cases: &[(&str, &[&str])] = &[
        (
            r#"{"schema":123,"run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#,
            &["process"],
        ),
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"exec","value":"/bin/sh"}"#,
            &["process"],
        ),
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"openat","value":"/tmp/a","flags":0,"mode":0,"return_value":3,"access_mode":"read","status":"success"}"#,
            &["filesystem"],
        ),
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":2,"kind":"sendto","value":"1.1.1.1:53"}"#,
            &["network"],
        ),
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":112,"kind":"inode_resolved","value":null}"#,
            &["filesystem"],
        ),
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":99,"kind":"event_99","value":null}"#,
            &["filesystem"],
        ),
        // openat missing required open fields
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a"}"#,
            &["filesystem"],
        ),
        // openat with status inconsistent with negative return_value
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":0,"mode":0,"return_value":-2,"access_mode":"read","status":"success"}"#,
            &["filesystem"],
        ),
        // openat with forbidden network field
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":0,"mode":0,"return_value":3,"access_mode":"read","status":"success","network_port":443}"#,
            &["filesystem"],
        ),
        // plain exec carrying openat flags
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh","flags":0}"#,
            &["process"],
        ),
        // connect_blocked with partial blocked-socket fields
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":20,"kind":"connect_blocked","value":"1.2.3.4:80","cgroup_id":10}"#,
            &["network"],
        ),
        // missing required "value" key
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec"}"#,
            &["process"],
        ),
        // unsafe run_id
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"../bad","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#,
            &["process"],
        ),
        // unknown extra field
        (
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh","extra":true}"#,
            &["process"],
        ),
    ];

    for (idx, (line, surfaces)) in invalid_shape_cases.iter().enumerate() {
        let pkg = single_line_kernel_package(line, surfaces);
        assert_eq!(
            verify_incident_package(&pkg, &ctx).reason,
            Some(IncidentReason::InputShape),
            "case {idx} must refuse with InputShape"
        );
    }
}

#[test]
fn kernel_rejects_invalid_or_mismatched_surface_declarations() {
    let ctx = ContextInput::default();
    let exec_line = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#;
    let open_line = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":0,"mode":420,"return_value":3,"access_mode":"read","status":"success"}"#;

    let mapping_cases: &[(&str, &[&str])] = &[
        // Disallowed surface for kernel format
        (exec_line, &["tool_call"]),
        (exec_line, &["transcript_join"]),
        // Non-canonical surface order or duplicate surfaces
        (open_line, &["network", "filesystem"]),
        (open_line, &["filesystem", "filesystem"]),
        // Non-empty kernel file whose observed surface disagrees with declared surface
        (exec_line, &["filesystem"]),
        // Non-empty kernel file declaring an extra unobserved surface
        (open_line, &["filesystem", "network"]),
        // Non-empty kernel file declaring empty surfaces
        (open_line, &[]),
    ];

    for (idx, (line, surfaces)) in mapping_cases.iter().enumerate() {
        let pkg = single_line_kernel_package(line, surfaces);
        assert_eq!(
            verify_incident_package(&pkg, &ctx).reason,
            Some(IncidentReason::Mapping),
            "mapping case {idx} must refuse with Mapping"
        );
    }
}

#[test]
fn kernel_rejects_truncated_blank_bom_lone_cr_duplicate_key_and_deep_lines() {
    let ctx = ContextInput::default();
    let valid_exec = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#;

    // Truncated JSON line
    let truncated = raw_kernel_bytes_package(
        br#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1""#,
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&truncated, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // Blank trailing line
    let blank_line =
        raw_kernel_bytes_package(format!("{valid_exec}\n\n").as_bytes(), &["process"], &ctx);
    assert_eq!(
        verify_incident_package(&blank_line, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // UTF-8 BOM prefix
    let bom_line = raw_kernel_bytes_package(
        format!("\u{feff}{valid_exec}\n").as_bytes(),
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&bom_line, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // Lone CR inside record
    let lone_cr = raw_kernel_bytes_package(
        format!("{valid_exec}\r{valid_exec}\n").as_bytes(),
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&lone_cr, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // Valid kernel JSON followed by terminal lone CR (no LF) -> InputShape
    let terminal_lone_cr =
        raw_kernel_bytes_package(format!("{valid_exec}\r").as_bytes(), &["process"], &ctx);
    assert_eq!(
        verify_incident_package(&terminal_lone_cr, &ctx).reason,
        Some(IncidentReason::InputShape)
    );
    let second_line_terminal_lone_cr = raw_kernel_bytes_package(
        format!("{valid_exec}\n{valid_exec}\r").as_bytes(),
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&second_line_terminal_lone_cr, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // Positive control: CRLF and final valid line without trailing newline stay valid
    let crlf_and_unterminated = format!("{valid_exec}\r\n{valid_exec}");
    let crlf_bytes = crlf_and_unterminated.as_bytes();
    let crlf_sha = sha256_hex(crlf_bytes);
    let crlf_val: Value = serde_json::from_str(valid_exec).unwrap();
    let mut crlf_units = vec![
        examined_unit(
            "kernel",
            &crlf_sha,
            &format!("line:0:0:{}", valid_exec.len()),
            "process",
            crlf_val.clone(),
        ),
        examined_unit(
            "kernel",
            &crlf_sha,
            &format!("line:1:{}:{}", valid_exec.len() + 2, valid_exec.len()),
            "process",
            crlf_val,
        ),
    ];
    sort_units_by_id(&mut crlf_units);
    let crlf_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": crlf_units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 2, 2)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let crlf_inputs = json!([{
        "id": "kernel",
        "format": KERNEL_FORMAT,
        "sha256": crlf_sha,
        "bytes": crlf_bytes.len() as u64,
        "surfaces": ["process"],
        "binding": null
    }]);
    let (crlf_pkg, _, _) = build_package(crlf_inputs, &crlf_assessment, &[crlf_bytes]);
    let crlf_report = verify_incident_package(&crlf_pkg, &ctx);
    assert_eq!(crlf_report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(crlf_report.reason, None);

    // Duplicate JSON key
    let dup_key = raw_kernel_bytes_package(
        br#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#,
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&dup_key, &ctx).reason,
        Some(IncidentReason::InputShape)
    );

    // Selected JSON depth exceeded on kernel line (metadata max depth is 4; limit=5 passes Phase 2,
    // fails Phase 5 on depth-6 kernel line)
    let mut selected_depth_ctx = ContextInput::default();
    selected_depth_ctx.limits.json_depth = 5;
    let depth_6 = raw_kernel_bytes_package(
        br#"{"a":{"b":{"c":{"d":{"e":{"f":1}}}}}}"#,
        &["process"],
        &selected_depth_ctx,
    );
    assert_eq!(
        verify_incident_package(&depth_6, &selected_depth_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // Hard JSON depth (64) exceeded on kernel line before materialization
    let open_brackets = "[".repeat(65);
    let close_brackets = "]".repeat(65);
    let depth_65 = raw_kernel_bytes_package(
        format!(r#"{{"a":{open_brackets}1{close_brackets}}}"#).as_bytes(),
        &["process"],
        &ctx,
    );
    assert_eq!(
        verify_incident_package(&depth_65, &ctx).reason,
        Some(IncidentReason::ResourceHard)
    );
}

#[test]
fn kernel_enforces_selected_and_hard_source_record_and_count_limits() {
    assert_eq!(ContextLimits::HARD.transcript_bytes, 16_777_216);
    assert_eq!(ContextLimits::HARD.record_bytes, 1_048_576);
    assert_eq!(ContextLimits::HARD.records, 100_000);

    let valid_exec = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#;
    let prefix = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":""#;
    let suffix = r#""}"#;
    let framing_len = prefix.len() + suffix.len();

    // 1. Selected and hard transcript_bytes (16 MiB = 16_777_216 bytes)
    let mut transcript_ctx = ContextInput::default();
    transcript_ctx.limits.transcript_bytes = (valid_exec.len() - 1) as u64;
    let transcript_pkg = raw_kernel_bytes_package(
        format!("{valid_exec}\n").as_bytes(),
        &["process"],
        &transcript_ctx,
    );
    assert_eq!(
        verify_incident_package(&transcript_pkg, &transcript_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // 32 lines of 524_288 bytes (524_287-byte line + LF) = 16_777_216 bytes (exact hard limit),
    // with each line <= 1 MiB record_bytes so only transcript_bytes is exercised at the boundary.
    let chunk_line_len = 524_287usize;
    let chunk_line = format!(
        "{prefix}{}{suffix}\n",
        "a".repeat(chunk_line_len - framing_len)
    );
    assert_eq!(chunk_line.len(), 524_288);
    let exact_16mib_bytes = chunk_line.repeat(32).into_bytes();
    assert_eq!(
        exact_16mib_bytes.len() as u64,
        ContextLimits::HARD.transcript_bytes
    );

    let default_ctx = ContextInput::default();
    let exact_16mib_pkg = raw_kernel_bytes_package(&exact_16mib_bytes, &["process"], &default_ctx);
    assert_eq!(
        verify_incident_package(&exact_16mib_pkg, &default_ctx).reason,
        Some(IncidentReason::Disclosure),
        "exact 16 MiB kernel source must pass Phase 5 resource guards and reach Phase 7 Disclosure"
    );

    let mut just_below_16mib_ctx = ContextInput::default();
    just_below_16mib_ctx.limits.transcript_bytes = ContextLimits::HARD.transcript_bytes - 1;
    let exact_16mib_selected_pkg =
        raw_kernel_bytes_package(&exact_16mib_bytes, &["process"], &just_below_16mib_ctx);
    assert_eq!(
        verify_incident_package(&exact_16mib_selected_pkg, &just_below_16mib_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    let mut over_16mib_bytes = chunk_line.repeat(31).into_bytes();
    let last_over_line = format!(
        "{prefix}{}{suffix}\n",
        "a".repeat((chunk_line_len + 1) - framing_len)
    );
    over_16mib_bytes.extend_from_slice(last_over_line.as_bytes());
    assert_eq!(
        over_16mib_bytes.len() as u64,
        ContextLimits::HARD.transcript_bytes + 1
    );

    let over_16mib_pkg = raw_kernel_bytes_package(&over_16mib_bytes, &["process"], &default_ctx);
    assert_eq!(
        verify_incident_package(&over_16mib_pkg, &default_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    let mut small_transcript_ctx = ContextInput::default();
    small_transcript_ctx.limits.transcript_bytes = 4096;
    let over_16mib_hard_first_pkg =
        raw_kernel_bytes_package(&over_16mib_bytes, &["process"], &small_transcript_ctx);
    assert_eq!(
        verify_incident_package(&over_16mib_hard_first_pkg, &small_transcript_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    let mut earlier_obj_total_ctx = ContextInput::default();
    earlier_obj_total_ctx.limits.object_bytes_total = ContextLimits::HARD.transcript_bytes;
    let over_16mib_obj_total_pkg =
        raw_kernel_bytes_package(&over_16mib_bytes, &["process"], &earlier_obj_total_ctx);
    assert_eq!(
        verify_incident_package(&over_16mib_obj_total_pkg, &earlier_obj_total_ctx).reason,
        Some(IncidentReason::ResourceSelected),
        "Phase 4 object_bytes_total check must precede Phase 5 transcript_bytes check"
    );

    // 2. Selected and hard record_bytes (1 MiB = 1_048_576 bytes)
    let mut record_ctx = ContextInput::default();
    record_ctx.limits.record_bytes = (valid_exec.len() - 1) as u64;
    let record_pkg = raw_kernel_bytes_package(
        format!("{valid_exec}\n").as_bytes(),
        &["process"],
        &record_ctx,
    );
    assert_eq!(
        verify_incident_package(&record_pkg, &record_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    let exact_1mib_record_len = ContextLimits::HARD.record_bytes as usize;
    let exact_1mib_line = format!(
        "{prefix}{}{suffix}",
        "a".repeat(exact_1mib_record_len - framing_len)
    );
    assert_eq!(
        exact_1mib_line.len() as u64,
        ContextLimits::HARD.record_bytes
    );
    let exact_1mib_pkg = single_line_kernel_package(&exact_1mib_line, &["process"]);
    let exact_1mib_report = verify_incident_package(&exact_1mib_pkg, &default_ctx);
    assert_eq!(exact_1mib_report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(exact_1mib_report.reason, None);

    let over_1mib_line = format!(
        "{prefix}{}{suffix}",
        "a".repeat((exact_1mib_record_len + 1) - framing_len)
    );
    assert_eq!(
        over_1mib_line.len() as u64,
        ContextLimits::HARD.record_bytes + 1
    );
    let over_1mib_pkg = raw_kernel_bytes_package(
        format!("{over_1mib_line}\n").as_bytes(),
        &["process"],
        &default_ctx,
    );
    assert_eq!(
        verify_incident_package(&over_1mib_pkg, &default_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    let mut small_record_ctx = ContextInput::default();
    small_record_ctx.limits.record_bytes = 1024;
    let over_1mib_hard_first_pkg = raw_kernel_bytes_package(
        format!("{over_1mib_line}\n").as_bytes(),
        &["process"],
        &small_record_ctx,
    );
    assert_eq!(
        verify_incident_package(&over_1mib_hard_first_pkg, &small_record_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    // 3. Selected and hard records count (100_000), including same-digest duplicate input charging
    let mut count_ctx = ContextInput::default();
    count_ctx.limits.records = 1;
    let two_lines_pkg = raw_kernel_bytes_package(
        format!("{valid_exec}\n{valid_exec}\n").as_bytes(),
        &["process"],
        &count_ctx,
    );
    assert_eq!(
        verify_incident_package(&two_lines_pkg, &count_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // Equal-digest kernel object declared twice in inventory.inputs charges records per occurrence:
    // within-bound at records=2 verifies, and records=1 refuses with ResourceSelected.
    let one_line_bytes = format!("{valid_exec}\n").into_bytes();
    let one_line_sha = sha256_hex(&one_line_bytes);
    let one_line_loc = format!("line:0:0:{}", valid_exec.len());
    let one_line_val: Value = serde_json::from_str(valid_exec).unwrap();
    let mut dup_digest_units = vec![
        examined_unit(
            "kernel-a",
            &one_line_sha,
            &one_line_loc,
            "process",
            one_line_val.clone(),
        ),
        examined_unit(
            "kernel-b",
            &one_line_sha,
            &one_line_loc,
            "process",
            one_line_val,
        ),
    ];
    sort_units_by_id(&mut dup_digest_units);
    let dup_digest_inputs = json!([
        {
            "id": "kernel-a",
            "format": KERNEL_FORMAT,
            "sha256": one_line_sha,
            "bytes": one_line_bytes.len() as u64,
            "surfaces": ["process"],
            "binding": null
        },
        {
            "id": "kernel-b",
            "format": KERNEL_FORMAT,
            "sha256": one_line_sha,
            "bytes": one_line_bytes.len() as u64,
            "surfaces": ["process"],
            "binding": null
        }
    ]);
    let mut count_two_ctx = ContextInput::default();
    count_two_ctx.limits.records = 2;
    let dup_digest_ok_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&count_two_ctx),
        "units": dup_digest_units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel-a", "kernel-b"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 2, 2)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let (dup_digest_ok_pkg, _, _) = build_package(
        dup_digest_inputs.clone(),
        &dup_digest_ok_assessment,
        &[&one_line_bytes],
    );
    let dup_digest_ok_report = verify_incident_package(&dup_digest_ok_pkg, &count_two_ctx);
    assert_eq!(
        dup_digest_ok_report.outcome,
        IncidentOutcome::PackageVerified
    );
    assert_eq!(dup_digest_ok_report.reason, None);

    let dup_digest_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&count_ctx),
        "units": [],
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel-a", "kernel-b"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 0, 0)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let (dup_digest_pkg, _, _) = build_package(
        dup_digest_inputs,
        &dup_digest_assessment,
        &[&one_line_bytes],
    );
    assert_eq!(
        verify_incident_package(&dup_digest_pkg, &count_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // Hard records (100_000): short valid records (112 bytes/line including LF) keep 100_001 lines
    // at ~11.2 MiB, well under the 16 MiB transcript_bytes ceiling.
    let short_exec = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"x"}"#;
    let short_exec_line = format!("{short_exec}\n");
    let exact_100k_bytes = short_exec_line
        .repeat(ContextLimits::HARD.records as usize)
        .into_bytes();
    assert!(
        (exact_100k_bytes.len() as u64) < ContextLimits::HARD.transcript_bytes,
        "100k short records must fit inside 16 MiB transcript_bytes"
    );
    let exact_100k_pkg = raw_kernel_bytes_package(&exact_100k_bytes, &["process"], &default_ctx);
    assert_eq!(
        verify_incident_package(&exact_100k_pkg, &default_ctx).reason,
        Some(IncidentReason::Disclosure),
        "exact 100k records must pass Phase 5 resource guards and reach Phase 7 Disclosure"
    );

    let mut over_100k_bytes = exact_100k_bytes;
    over_100k_bytes.extend_from_slice(short_exec_line.as_bytes());
    assert!(
        (over_100k_bytes.len() as u64) < ContextLimits::HARD.transcript_bytes,
        "100,001 short records must fit inside 16 MiB transcript_bytes"
    );
    let over_100k_pkg = raw_kernel_bytes_package(&over_100k_bytes, &["process"], &default_ctx);
    assert_eq!(
        verify_incident_package(&over_100k_pkg, &default_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    // Streaming charge_record evaluates each record in order, so selected records=50_000
    // refuses with ResourceSelected on record 50,001 before reaching record 100,001.
    let mut small_count_ctx = ContextInput::default();
    small_count_ctx.limits.records = 50_000;
    let over_100k_selected_streaming_pkg =
        raw_kernel_bytes_package(&over_100k_bytes, &["process"], &small_count_ctx);
    assert_eq!(
        verify_incident_package(&over_100k_selected_streaming_pkg, &small_count_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // Aggregate 100,001 records across separately declared inputs (two 50,000-line same-digest
    // inputs + one 1-line input) -> ResourceHard.
    let half_100k_bytes = short_exec_line.repeat(50_000).into_bytes();
    let half_100k_sha = sha256_hex(&half_100k_bytes);
    let agg_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&default_ctx),
        "units": [],
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row(
                "process",
                &["kernel-a", "kernel-b", "kernel-c"],
                "unknown",
                json!([]),
                Value::Null
            ),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 0, 0)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let agg_inputs = json!([
        {
            "id": "kernel-a",
            "format": KERNEL_FORMAT,
            "sha256": half_100k_sha,
            "bytes": half_100k_bytes.len() as u64,
            "surfaces": ["process"],
            "binding": null
        },
        {
            "id": "kernel-b",
            "format": KERNEL_FORMAT,
            "sha256": half_100k_sha,
            "bytes": half_100k_bytes.len() as u64,
            "surfaces": ["process"],
            "binding": null
        },
        {
            "id": "kernel-c",
            "format": KERNEL_FORMAT,
            "sha256": one_line_sha,
            "bytes": one_line_bytes.len() as u64,
            "surfaces": ["process"],
            "binding": null
        }
    ]);
    let (agg_over_100k_pkg, _, _) = build_package(
        agg_inputs,
        &agg_assessment,
        &[&half_100k_bytes, &one_line_bytes],
    );
    assert_eq!(
        verify_incident_package(&agg_over_100k_pkg, &default_ctx).reason,
        Some(IncidentReason::ResourceHard)
    );

    // 4. Selected payload_string_bytes limit on kernel "value" field
    let mut payload_ctx = ContextInput::default();
    payload_ctx.limits.payload_string_bytes = 4;
    let payload_pkg = raw_kernel_bytes_package(
        format!("{valid_exec}\n").as_bytes(),
        &["process"],
        &payload_ctx,
    );
    assert_eq!(
        verify_incident_package(&payload_pkg, &payload_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );

    // 5. Selected non_payload_string_bytes limit on kernel "run_id" field (metadata max is 64-byte hex)
    let mut non_payload_ctx = ContextInput::default();
    non_payload_ctx.limits.non_payload_string_bytes = 80;
    let long_run_id = "r".repeat(96);
    let long_non_payload_line = format!(
        r#"{{"schema":"{KERNEL_SCHEMA}","run_id":"{long_run_id}","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}}"#
    );
    let non_payload_pkg = raw_kernel_bytes_package(
        format!("{long_non_payload_line}\n").as_bytes(),
        &["process"],
        &non_payload_ctx,
    );
    assert_eq!(
        verify_incident_package(&non_payload_pkg, &non_payload_ctx).reason,
        Some(IncidentReason::ResourceSelected)
    );
}

#[test]
fn kernel_rejects_digest_locator_and_disclosure_mismatches() {
    let ctx = ContextInput::default();
    let line0 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"r1","seq":0,"pid":1,"event_type":4,"kind":"exec","value":"/bin/sh"}"#;
    let raw_two_lines = format!("{line0}\n{line0}\n").into_bytes();
    let sha = sha256_hex(&raw_two_lines);
    let loc0 = format!("line:0:0:{}", line0.len());
    let loc1 = format!("line:1:{}:{}", line0.len() + 1, line0.len());
    let val: Value = serde_json::from_str(line0).unwrap();

    // 1. Tampered object bytes vs inventory digest -> Digest
    let tampered_bytes = format!("{line0}\n{line0} \n").into_bytes();
    let mut units = vec![
        examined_unit("kernel", &sha, &loc0, "process", val.clone()),
        examined_unit("kernel", &sha, &loc1, "process", val.clone()),
    ];
    sort_units_by_id(&mut units);
    let base_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &[], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("process", 2, 2)]),
        "joins": [],
        "claims": [],
        "resolved_results": []
    });
    let inputs = json!([{
        "id": "kernel",
        "format": KERNEL_FORMAT,
        "sha256": sha,
        "bytes": raw_two_lines.len() as u64,
        "surfaces": ["process"],
        "binding": null
    }]);
    let (digest_pkg, _, _) = build_package(inputs.clone(), &base_assessment, &[&tampered_bytes]);
    assert_eq!(
        verify_incident_package(&digest_pkg, &ctx).reason,
        Some(IncidentReason::Digest)
    );

    // 2. Off-by-one byte-length in line locator -> Locator
    let bad_loc1 = format!("line:1:{}:{}", line0.len() + 1, line0.len() + 1);
    let mut bad_loc_units = vec![
        examined_unit("kernel", &sha, &loc0, "process", val.clone()),
        examined_unit("kernel", &sha, &bad_loc1, "process", val.clone()),
    ];
    sort_units_by_id(&mut bad_loc_units);
    let mut bad_loc_assessment = base_assessment.clone();
    bad_loc_assessment["units"] = json!(bad_loc_units);
    let (locator_pkg, _, _) = build_package(inputs.clone(), &bad_loc_assessment, &[&raw_two_lines]);
    assert_eq!(
        verify_incident_package(&locator_pkg, &ctx).reason,
        Some(IncidentReason::Locator)
    );

    // 3. Omitting second duplicate raw line occurrence -> Disclosure
    let mut omitted_assessment = base_assessment.clone();
    omitted_assessment["units"] =
        json!([examined_unit("kernel", &sha, &loc0, "process", val.clone())]);
    let (omitted_pkg, _, _) = build_package(inputs.clone(), &omitted_assessment, &[&raw_two_lines]);
    assert_eq!(
        verify_incident_package(&omitted_pkg, &ctx).reason,
        Some(IncidentReason::Disclosure)
    );

    // 4. Tampered inline disclosed result value -> Disclosure
    let mut tampered_val = val;
    tampered_val["pid"] = json!(999);
    let mut tampered_units = vec![
        examined_unit("kernel", &sha, &loc0, "process", tampered_val),
        examined_unit(
            "kernel",
            &sha,
            &loc1,
            "process",
            serde_json::from_str(line0).unwrap(),
        ),
    ];
    sort_units_by_id(&mut tampered_units);
    let mut tampered_assessment = base_assessment;
    tampered_assessment["units"] = json!(tampered_units);
    let (disclosure_pkg, _, _) = build_package(inputs, &tampered_assessment, &[&raw_two_lines]);
    assert_eq!(
        verify_incident_package(&disclosure_pkg, &ctx).reason,
        Some(IncidentReason::Disclosure)
    );
}

#[test]
fn kernel_rejects_strong_joins_and_absence_or_exhaustive_overclaims() {
    let ctx = ContextInput::default();
    let line0 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_join","seq":0,"pid":10,"event_type":1,"kind":"openat","value":"/tmp/a","flags":0,"mode":420,"return_value":3,"access_mode":"read","status":"success"}"#;
    let line1 = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_join","seq":1,"pid":10,"event_type":1,"kind":"openat","value":"/tmp/b","flags":0,"mode":420,"return_value":4,"access_mode":"read","status":"success"}"#;
    let raw_bytes = format!("{line0}\n{line1}\n").into_bytes();
    let sha = sha256_hex(&raw_bytes);
    let loc0 = format!("line:0:0:{}", line0.len());
    let loc1 = format!("line:1:{}:{}", line0.len() + 1, line1.len());

    let mut units = vec![
        examined_unit(
            "kernel",
            &sha,
            &loc0,
            "filesystem",
            serde_json::from_str(line0).unwrap(),
        ),
        examined_unit(
            "kernel",
            &sha,
            &loc1,
            "filesystem",
            serde_json::from_str(line1).unwrap(),
        ),
    ];
    sort_units_by_id(&mut units);

    let inputs = json!([{
        "id": "kernel",
        "format": KERNEL_FORMAT,
        "sha256": sha,
        "bytes": raw_bytes.len() as u64,
        "surfaces": ["filesystem"],
        "binding": null
    }]);

    // 1. Overclaiming a strong / confirmed run_id join on raw kernel records -> ClaimBoundary
    let strong_join = json!({
        "id": "join-kernel-run",
        "left": { "input_id": "kernel", "sha256": sha, "locator": loc0 },
        "right": { "input_id": "kernel", "sha256": sha, "locator": loc1 },
        "contract": {
            "schema": "assay.observability.join_result.v0",
            "left_artifact_role": "external_receipt",
            "right_artifact_role": "external_receipt",
            "join_key": "run_id",
            "join_value": "run_join",
            "join_grade": "strong",
            "scope": "run",
            "unique_within_scope": true,
            "fallback_used": false,
            "evidence_refs": [
                format!("kernel#{loc0}"),
                format!("kernel#{loc1}")
            ],
            "notes": []
        },
        "proposition": {
            "kind": "equal_retained_value",
            "left_pointer": "/run_id",
            "right_pointer": "/run_id",
            "expected": null,
            "surface": "filesystem"
        },
        "result": "confirmed"
    });
    let strong_join_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("filesystem", 2, 2)]),
        "joins": [strong_join],
        "claims": [],
        "resolved_results": []
    });
    let (strong_join_pkg, _, _) =
        build_package(inputs.clone(), &strong_join_assessment, &[&raw_bytes]);
    assert_eq!(
        verify_incident_package(&strong_join_pkg, &ctx).reason,
        Some(IncidentReason::ClaimBoundary)
    );

    // 2. Honest weak/unobservable join, but overclaiming exhaustive_set or bounded_negative
    // support on raw kernel surface -> ClaimBoundary
    let weak_join = json!({
        "id": "join-kernel-weak",
        "left": { "input_id": "kernel", "sha256": sha, "locator": loc0 },
        "right": { "input_id": "kernel", "sha256": sha, "locator": loc1 },
        "contract": {
            "schema": "assay.observability.join_result.v0",
            "left_artifact_role": "external_receipt",
            "right_artifact_role": "external_receipt",
            "join_key": "run_id",
            "join_value": "run_join",
            "join_grade": "weak",
            "scope": "run",
            "unique_within_scope": false,
            "fallback_used": false,
            "evidence_refs": [
                format!("kernel#{loc0}"),
                format!("kernel#{loc1}")
            ],
            "notes": []
        },
        "proposition": {
            "kind": "equal_retained_value",
            "left_pointer": "/run_id",
            "right_pointer": "/run_id",
            "expected": null,
            "surface": "filesystem"
        },
        "result": "unobservable"
    });
    let claim_evidence_refs = json!([
        { "input_id": "kernel", "sha256": sha, "locator": loc0 },
        { "input_id": "kernel", "sha256": sha, "locator": loc1 }
    ]);

    let exhaustive_overclaim_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("filesystem", 2, 2)]),
        "joins": [weak_join.clone()],
        "claims": [{
            "id": "claim-exhaustive",
            "surface": "filesystem",
            "stratum": "filesystem",
            "kind": "exhaustive_set",
            "proposition": "join-kernel-weak",
            "evidence_refs": claim_evidence_refs.clone(),
            "decision": "supported"
        }],
        "resolved_results": []
    });
    let (exhaustive_pkg, _, _) = build_package(
        inputs.clone(),
        &exhaustive_overclaim_assessment,
        &[&raw_bytes],
    );
    assert_eq!(
        verify_incident_package(&exhaustive_pkg, &ctx).reason,
        Some(IncidentReason::ClaimBoundary)
    );

    let absence_overclaim_assessment = json!({
        "schema": "assay.incident.assessment.v1",
        "verification_context_sha256": context_sha256_hex(&ctx),
        "units": units,
        "surfaces": [
            surface_row("tool_call", &[], "unknown", json!([]), Value::Null),
            surface_row("filesystem", &["kernel"], "unknown", json!([]), Value::Null),
            surface_row("network", &[], "unknown", json!([]), Value::Null),
            surface_row("process", &[], "unknown", json!([]), Value::Null),
            surface_row("transcript_join", &[], "unknown", json!([]), Value::Null)
        ],
        "cap1": cap1_document(vec![cap1_stratum("filesystem", 2, 2)]),
        "joins": [weak_join],
        "claims": [{
            "id": "claim-absence",
            "surface": "filesystem",
            "stratum": "filesystem",
            "kind": "bounded_negative",
            "proposition": "join-kernel-weak",
            "evidence_refs": claim_evidence_refs,
            "decision": "supported"
        }],
        "resolved_results": []
    });
    let (absence_pkg, _, _) = build_package(inputs, &absence_overclaim_assessment, &[&raw_bytes]);
    assert_eq!(
        verify_incident_package(&absence_pkg, &ctx).reason,
        Some(IncidentReason::ClaimBoundary)
    );
}
