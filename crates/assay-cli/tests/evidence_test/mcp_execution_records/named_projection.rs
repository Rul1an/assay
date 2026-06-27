use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

use super::fixtures::{decision_json, jcs_digest_value};

fn named_digest(params: &Value, binding: &Value) -> String {
    jcs_digest_value(&serde_json::json!({
        "projection": "assay.fallback_projection.v0",
        "params": params,
        "binding": binding,
    }))
}

fn named_envelope(params: &Value, binding: Option<&Value>, extra_meta: &[(&str, Value)]) -> String {
    let mut meta = serde_json::Map::new();
    if let Some(b) = binding {
        meta.insert("authorization_binding".to_string(), b.clone());
    }
    for (k, v) in extra_meta {
        meta.insert((*k).to_string(), v.clone());
    }
    serde_json::json!({ "params": params, "_meta": Value::Object(meta) }).to_string()
}

fn sample_params() -> Value {
    serde_json::json!({ "name": "tools/call", "arguments": { "processInstanceKey": "2251799813685249" } })
}

fn sample_binding() -> Value {
    serde_json::json!({ "tenant": "acme", "resource": "provider/customer/cus_123" })
}

#[test]
fn fallback_named_projection_excludes_nonbinding_meta() {
    let dir = tempdir().unwrap();
    let params = sample_params();
    let binding = sample_binding();
    // Envelope carries extra, non-binding _meta the digest must NOT depend on.
    let envelope = named_envelope(
        &params,
        Some(&binding),
        &[
            ("progress_token", serde_json::json!("p-001")),
            (
                "trace_context",
                serde_json::json!({ "traceparent": "00-abc" }),
            ),
        ],
    );
    // The expected digest is computed from params + binding only.
    let digest = named_digest(&params, &binding);
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest)).unwrap();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["binding"]["mode"], "request_envelope");
    assert_eq!(report["binding"]["digest"], digest);
    assert_eq!(
        report["binding"]["digest_source"],
        "request_envelope_named_projection_jcs"
    );
    assert_eq!(
        report["binding"]["projection"],
        "assay.fallback_projection.v0"
    );
}

#[test]
fn fallback_named_projection_same_digest_for_different_nonbinding_meta() {
    // Two envelopes that differ only in non-binding _meta must produce the same binding digest.
    let dir = tempdir().unwrap();
    let params = sample_params();
    let binding = sample_binding();
    let digest = named_digest(&params, &binding);
    let decision = dir.path().join("decision.json");
    fs::write(&decision, decision_json(&digest)).unwrap();

    for (i, extra) in [
        ("gateway", serde_json::json!("gw-token")),
        ("provider", serde_json::json!("pv-trace")),
    ]
    .into_iter()
    .enumerate()
    {
        let env_path = dir.path().join(format!("env-{i}.json"));
        fs::write(
            &env_path,
            named_envelope(&params, Some(&binding), &[(extra.0, extra.1)]),
        )
        .unwrap();
        let output = Command::cargo_bin("assay")
            .unwrap()
            .args([
                "evidence",
                "verify-mcp-records",
                "--request-envelope",
                env_path.to_str().unwrap(),
                "--decision",
                decision.to_str().unwrap(),
                "--fallback-projection",
                "named",
                "--format",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let report: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            report["binding"]["digest"], digest,
            "non-binding _meta must not change the digest"
        );
    }
}

#[test]
fn fallback_named_projection_fails_closed_when_binding_block_absent() {
    let dir = tempdir().unwrap();
    let params = sample_params();
    // Envelope with NO _meta.authorization_binding.
    let envelope = named_envelope(&params, None, &[("progress_token", serde_json::json!("p"))]);
    let digest = named_digest(&params, &sample_binding());
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "fallback_projection_missing_authorization_binding",
        ))
        .stdout(predicate::str::contains("failing closed"));
}

#[test]
fn fallback_named_projection_invalid_meta_fails_closed() {
    let dir = tempdir().unwrap();
    // `_meta` present but not an object -> invalid, distinct reason code from a missing binding.
    let envelope =
        serde_json::json!({ "params": sample_params(), "_meta": "not-an-object" }).to_string();
    let digest = named_digest(&sample_params(), &sample_binding());
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("fallback_projection_invalid_meta"));
}

#[test]
fn fallback_named_projection_id_is_bound_in_the_digest() {
    // Changing the projection id changes the digest: a decision whose backLink was computed with a
    // DIFFERENT projection id must not pair, proving the projection id is part of the preimage.
    let dir = tempdir().unwrap();
    let params = sample_params();
    let binding = sample_binding();
    let envelope = named_envelope(&params, Some(&binding), &[]);
    // Digest computed with a WRONG projection id; the verifier uses the real id, so they differ.
    let wrong = jcs_digest_value(&serde_json::json!({
        "projection": "assay.fallback_projection.v0-WRONG",
        "params": params,
        "binding": binding,
    }));
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&wrong)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_request_envelope_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn fallback_named_projection_binds_whole_authorization_binding() {
    // Bind-all: an extra field inside authorization_binding is part of the preimage, so a decision
    // built without that field does not pair. (Documents that there is no allowlist *inside* the
    // binding block; the whole object is bound.)
    let dir = tempdir().unwrap();
    let params = sample_params();
    let binding_without = sample_binding();
    let mut binding_with = sample_binding();
    binding_with["extra_inner_field"] = serde_json::json!("x");
    let envelope = named_envelope(&params, Some(&binding_with), &[]);
    let digest_without = named_digest(&params, &binding_without);
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest_without)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_request_envelope_digest_match",
        ));
}

#[test]
fn fallback_named_projection_breaks_on_changed_binding_block() {
    let dir = tempdir().unwrap();
    let params = sample_params();
    // Decision binds the ORIGINAL binding; the envelope carries a DIFFERENT binding.
    let digest = named_digest(&params, &sample_binding());
    let other_binding =
        serde_json::json!({ "tenant": "acme", "resource": "provider/customer/cus_999" });
    let envelope = named_envelope(&params, Some(&other_binding), &[]);
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_request_envelope_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}

#[test]
fn fallback_named_projection_breaks_on_changed_bound_param() {
    let dir = tempdir().unwrap();
    let binding = sample_binding();
    // Decision binds the ORIGINAL params; the envelope carries DIFFERENT params.
    let digest = named_digest(&sample_params(), &binding);
    let other_params =
        serde_json::json!({ "name": "tools/call", "arguments": { "processInstanceKey": "9999" } });
    let envelope = named_envelope(&other_params, Some(&binding), &[]);
    let env_path = dir.path().join("request-envelope.json");
    let decision = dir.path().join("decision.json");
    fs::write(&env_path, envelope).unwrap();
    fs::write(&decision, decision_json(&digest)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-records",
            "--request-envelope",
            env_path.to_str().unwrap(),
            "--decision",
            decision.to_str().unwrap(),
            "--fallback-projection",
            "named",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains(
            "decision_request_envelope_digest_match",
        ))
        .stdout(predicate::str::contains("fail mismatch"));
}
