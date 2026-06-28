use std::fs;
use std::process::Command;

use gateway_evidence_replay::schema::{Ceiling, NonClaim, Reason, Status};
use gateway_evidence_replay::verify_json_value;
use serde::Deserialize;
use serde_json::{json, Value};

const VECTORS: &str = include_str!("../fixtures/gateway-path-v0/vectors.json");

#[derive(Debug, Deserialize)]
struct VectorFile {
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    vector_id: String,
    inputs: Value,
    expected_status: Status,
    expected_ceiling: Option<Ceiling>,
    expected_reasons: Vec<Reason>,
}

#[test]
fn all_gateway_path_vectors_reproduce() {
    let vectors: VectorFile = serde_json::from_str(VECTORS).expect("vectors parse");
    assert_eq!(vectors.vectors.len(), 17);

    for vector in vectors.vectors {
        let got = verify_json_value(vector.inputs);
        assert_eq!(got.status, vector.expected_status, "{}", vector.vector_id);
        assert_eq!(got.ceiling, vector.expected_ceiling, "{}", vector.vector_id);
        assert_eq!(got.reasons, vector.expected_reasons, "{}", vector.vector_id);
    }
}

#[test]
fn cli_verifies_clean_route_as_json() {
    let bin = env!("CARGO_BIN_EXE_gateway-evidence-replay");
    let fixture = fixture_path("clean-route.json");
    let output = Command::new(bin)
        .args([
            "verify",
            fixture.to_str().unwrap(),
            "--format",
            "gateway-path.v0",
            "--json",
        ])
        .output()
        .expect("run gateway-evidence-replay");

    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(body["profile"], "gateway-path.v0");
    assert_eq!(body["status"], "path_verified");
    assert_eq!(body["ceiling"], "observed_in_path");
    assert_eq!(body["reasons"], json!([]));
}

#[test]
fn malformed_json_emits_invalid_result() {
    let bin = env!("CARGO_BIN_EXE_gateway-evidence-replay");
    let dir = tempfile::tempdir().expect("tempdir");
    let evidence = dir.path().join("broken.json");
    fs::write(&evidence, "{not-json").expect("write malformed evidence");

    let output = Command::new(bin)
        .args([
            "verify",
            evidence.to_str().unwrap(),
            "--format",
            "gateway-path.v0",
            "--json",
        ])
        .output()
        .expect("run gateway-evidence-replay");

    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(body["status"], "invalid");
    assert_eq!(body["reasons"], json!(["malformed_input"]));
}

#[test]
fn unknown_fields_fail_closed() {
    let mut input: Value =
        serde_json::from_str(include_str!("../fixtures/gateway-path-v0/clean-route.json"))
            .expect("clean fixture");
    input["provider_honest"] = json!(true);

    let got = verify_json_value(input);
    assert_eq!(got.status, Status::Invalid);
    assert_eq!(got.reasons, vec![Reason::MalformedInput]);
}

#[test]
fn malformed_timestamp_fails_closed() {
    let mut input: Value =
        serde_json::from_str(include_str!("../fixtures/gateway-path-v0/clean-route.json"))
            .expect("clean fixture");
    input["now"] = json!("2026-06-28 10:00:00");

    let got = verify_json_value(input);
    assert_eq!(got.status, Status::Invalid);
    assert_eq!(got.reasons, vec![Reason::MalformedInput]);
}

#[test]
fn unknown_source_class_is_reported_explicitly() {
    let mut input: Value =
        serde_json::from_str(include_str!("../fixtures/gateway-path-v0/clean-route.json"))
            .expect("clean fixture");
    input["source_class"] = json!("mystery_box");

    let got = verify_json_value(input);
    assert_eq!(got.status, Status::Invalid);
    assert_eq!(got.reasons, vec![Reason::UnknownSourceClass]);
}

#[test]
fn output_keeps_claim_ceiling_explicit() {
    let input: Value =
        serde_json::from_str(include_str!("../fixtures/gateway-path-v0/clean-route.json"))
            .expect("clean fixture");
    let got = verify_json_value(input);

    assert_eq!(got.non_claims, NonClaim::all());
    let output = serde_json::to_value(got).expect("serialize result");
    assert_eq!(output.get("provider_honest"), None);
    assert_eq!(output.get("response_true"), None);
    assert_eq!(output.get("gateway_enforced"), None);
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/gateway-path-v0/"
    ))
    .join(name)
}
