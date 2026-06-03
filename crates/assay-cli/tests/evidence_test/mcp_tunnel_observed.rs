use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

const VALID_TUNNEL: &str = "examples/mcp-tunnel-observed-evidence/fixtures/valid.tunnel.json";

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read_valid_tunnel() -> Value {
    let body = fs::read_to_string(repo_path(VALID_TUNNEL)).unwrap();
    serde_json::from_str(&body).unwrap()
}

fn write_fixture(value: &Value, name: &str) -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let path = dir.path().join(name);
    fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    (dir, path)
}

#[test]
fn verify_mcp_tunnel_observed_reports_strong_join_boundary() {
    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-tunnel-observed",
            "--artifact",
            repo_path(VALID_TUNNEL).to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema"], "assay.mcp.tunnel-observed.report.v0");
    assert_eq!(report["ok"], true);
    assert_eq!(report["verification_scope"]["role"], "independent-consumer");
    assert_eq!(
        report["request_binding"]["request_envelope_digest"],
        "sha256:1111111111111111111111111111111111111111111111111111111111111111"
    );
    assert!(report["request_binding"].get("route").is_none());
    assert!(report["request_binding"].get("upstream").is_none());
    assert_eq!(report["join_summary"]["strong_same_request_instance"], 1);
    assert_eq!(report["join_summary"]["diagnostic_correlation"], 0);
    assert!(report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| claim == "route_or_transport_mediation_proof"));
}

#[test]
fn verify_mcp_tunnel_observed_rejects_raw_payload_retention() {
    let mut artifact = read_valid_tunnel();
    artifact["visibility"]["raw_payload_retained"] = Value::Bool(true);
    let (_dir, path) = write_fixture(&artifact, "raw-payload-retained.tunnel.json");

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-tunnel-observed",
            "--artifact",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], false);
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["id"] == "raw_payload_not_retained" && check["ok"] == false));
}

#[test]
fn verify_mcp_tunnel_observed_classifies_route_only_ref_as_diagnostic() {
    let mut artifact = read_valid_tunnel();
    let refs = artifact["evidence_refs"].as_array_mut().unwrap();
    refs[0] = serde_json::json!({
        "kind": "mcp.execution_record",
        "digest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
        "relationship": "route_label_only",
        "join_strength": "diagnostic"
    });
    let (_dir, path) = write_fixture(&artifact, "route-only-diagnostic.tunnel.json");

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-tunnel-observed",
            "--artifact",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["join_summary"]["strong_same_request_instance"], 0);
    assert_eq!(report["join_summary"]["diagnostic_correlation"], 1);
}

#[test]
fn verify_mcp_tunnel_observed_rejects_mismatched_strong_join() {
    let mut artifact = read_valid_tunnel();
    artifact["evidence_refs"][0]["request_envelope_canonicalization"] =
        Value::String("json:mcp_request_envelope.unstable".to_string());
    let (_dir, path) = write_fixture(&artifact, "mismatched-strong-join.tunnel.json");

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-tunnel-observed",
            "--artifact",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["id"] == "same_request_instance_strong_join" && check["ok"] == false));
}
