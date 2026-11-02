//! Original sidecars are retained context, not action or join evidence.
use assay_evidence::{
    prepare_verified_incident_package, verify_incident_package, ContextInput, IncidentOutcome,
    IncidentPrepareSource, IncidentReason,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn observation(sequence: u64, kind: &str, link: &str) -> Value {
    let line = r#"{"jsonrpc":"2.0","id":"ask-1","result":{"action":"decline"}}"#;
    json!({"schema":"assay.request_observation.v0", "assertion_basis":"observed_on_wire",
        "kind":kind,"scope":"capture-a","sequence":sequence,"request_line":line,
        "request_bytes":line.len(),"request_sha256":hex::encode(Sha256::digest(line.as_bytes())),
        "rpc_id_present":true,"rpc_id":"ask-1","call_link":link,"call_id":null})
}
fn source(records: &[Value]) -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "sidecar".into(),
        format: "assay-request-receipts-ndjson-v0".into(),
        bytes: records
            .iter()
            .map(|v| format!("{v}\n"))
            .collect::<String>()
            .into_bytes(),
    }
}
#[test]
fn observation_context_preserves_bytes_without_actions_or_joins() {
    let src = source(&[
        observation(0, "ask", "ambiguous"),
        observation(1, "answer", "unlinked"),
    ]);
    let (package, report) =
        prepare_verified_incident_package(std::slice::from_ref(&src), &ContextInput::default())
            .unwrap();
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(
        verify_incident_package(&package, &ContextInput::default()).outcome,
        IncidentOutcome::PackageVerified
    );
    let mut archive = tar::Archive::new(std::io::Cursor::new(package));
    let mut found = false;
    for item in archive.entries().unwrap() {
        let mut item = item.unwrap();
        let name = item.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut item, &mut bytes).unwrap();
        if name == "assessment.json" {
            let v: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["units"], json!([]));
            assert_eq!(v["joins"], json!([]));
        }
        found |= bytes == src.bytes;
    }
    assert!(found);
}
#[test]
fn malformed_sidecars_refuse_through_public_prepare() {
    for (field, value) in [
        ("schema", json!("unknown")),
        ("request_sha256", json!("0".repeat(64))),
        ("request_bytes", json!(999)),
        ("sequence", json!(1)),
        ("kind", json!("approval")),
        ("call_link", json!("trusted")),
        ("call_id", json!("unexpected")),
        ("rpc_id_present", json!("true")),
        ("assertion_basis", json!("verified")),
        ("extra", json!(true)),
    ] {
        let mut v = observation(0, "ask", "ambiguous");
        v[field] = value;
        let error = prepare_verified_incident_package(&[source(&[v])], &ContextInput::default())
            .unwrap_err();
        assert_eq!(
            error.reason,
            Some(if field == "schema" {
                IncidentReason::UnknownSchema
            } else {
                IncidentReason::InputShape
            }),
            "{field}"
        );
    }
    let mut foreign = observation(1, "answer", "unlinked");
    foreign["scope"] = json!("other");
    assert_eq!(
        prepare_verified_incident_package(
            &[source(&[observation(0, "ask", "unlinked"), foreign])],
            &ContextInput::default()
        )
        .unwrap_err()
        .reason,
        Some(IncidentReason::InputShape)
    );
}

#[test]
fn sidecar_bounds_apply_before_payload_materialization() {
    let mut record = observation(0, "ask", "unlinked");
    let line = "x".repeat(70_000);
    record["request_line"] = json!(line);
    record["request_bytes"] = json!(line.len());
    record["request_sha256"] = json!(hex::encode(Sha256::digest(line.as_bytes())));
    assert!(prepare_verified_incident_package(
        &[source(std::slice::from_ref(&record))],
        &ContextInput::default()
    )
    .is_ok());
    let mut context = ContextInput::default();
    context.limits.record_bytes = 100;
    assert_eq!(
        prepare_verified_incident_package(&[source(std::slice::from_ref(&record))], &context)
            .unwrap_err()
            .reason,
        Some(IncidentReason::ResourceSelected)
    );
    let line = "\"".repeat(600_000);
    record["request_line"] = json!(line);
    record["request_bytes"] = json!(line.len());
    record["request_sha256"] = json!(hex::encode(Sha256::digest(line.as_bytes())));
    assert_eq!(
        prepare_verified_incident_package(&[source(&[record])], &ContextInput::default())
            .unwrap_err()
            .reason,
        Some(IncidentReason::ResourceHard)
    );
}

#[test]
fn empty_truncated_duplicate_key_sidecars_refuse() {
    let valid = source(&[observation(0, "ask", "unlinked")]);
    for bytes in [
        Vec::new(),
        b"{\"schema\":".to_vec(),
        String::from_utf8(valid.bytes.clone())
            .unwrap()
            .replacen("{", "{\"schema\":\"unknown\",", 1)
            .into_bytes(),
    ] {
        let src = IncidentPrepareSource {
            bytes,
            ..valid.clone()
        };
        assert_eq!(
            prepare_verified_incident_package(&[src], &ContextInput::default())
                .unwrap_err()
                .reason,
            Some(IncidentReason::InputShape)
        );
    }
}

#[test]
fn repaired_outer_digest_does_not_bypass_sidecar_validation_on_verify_or_export() {
    use assay_evidence::incident_package::verify::build_verified_incident_package;
    use assay_evidence::incident_package::write_incident_container;
    let src = source(&[observation(0, "ask", "unlinked")]);
    let (package, _) = prepare_verified_incident_package(&[src], &ContextInput::default()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(package));
    let mut inventory = Value::Null;
    let mut assessment = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        match name.as_str() {
            "inventory.json" => inventory = serde_json::from_slice(&bytes).unwrap(),
            "assessment.json" => assessment = bytes,
            _ => {}
        }
    }
    let mut invalid = observation(0, "ask", "unlinked");
    invalid["schema"] = json!("not.admitted");
    let bytes = source(&[invalid]).bytes;
    inventory["inputs"][0]["sha256"] = json!(hex::encode(Sha256::digest(&bytes)));
    inventory["inputs"][0]["bytes"] = json!(bytes.len());
    let mut inventory = assay_canonical::jcs::to_vec(&inventory).unwrap();
    inventory.push(b'\n');
    let package = write_incident_container(&inventory, &assessment, &[&bytes]).unwrap();
    assert_eq!(
        verify_incident_package(&package, &ContextInput::default()).reason,
        Some(IncidentReason::UnknownSchema)
    );
    assert_eq!(
        build_verified_incident_package(
            &inventory,
            &assessment,
            &[&bytes],
            &ContextInput::default()
        )
        .unwrap_err()
        .reason,
        Some(IncidentReason::UnknownSchema)
    );
}
