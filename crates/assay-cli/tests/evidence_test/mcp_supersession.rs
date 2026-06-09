//! Contract tests for `assay evidence verify-mcp-supersession`.
//!
//! Latest `decidedAt` wins; an equal-`decidedAt` tie with no explicit ordering field is ambiguous /
//! non-conformant (the verifier refuses to guess from order or nonce). An explicit `sequence` field
//! resolves a tie deterministically.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

fn decision(
    digest: &str,
    nonce: &str,
    decided_at: &str,
    decision: &str,
    sequence: Option<i64>,
) -> Value {
    let mut derived = serde_json::json!({ "decidedAt": decided_at, "decision": decision });
    if let Some(s) = sequence {
        derived["sequence"] = serde_json::json!(s);
    }
    serde_json::json!({
        "backLink": { "attestationDigest": digest, "attestationNonce": nonce },
        "decisionDerived": derived,
    })
}

fn run(records: &[Value]) -> assert_cmd::assert::Assert {
    let dir = tempdir().unwrap();
    let path = dir.path().join("decisions.json");
    fs::write(
        &path,
        serde_json::to_string(&Value::Array(records.to_vec())).unwrap(),
    )
    .unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "verify-mcp-supersession",
            "--decisions",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
}

#[test]
fn single_decision_resolves() {
    let out = run(&[decision(
        "sha256:aa",
        "n1",
        "2026-06-09T10:00:00Z",
        "allow",
        None,
    )])
    .success()
    .get_output()
    .stdout
    .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["groups"][0]["verdict"], "resolved");
}

#[test]
fn distinct_decided_at_latest_wins() {
    let out = run(&[
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "escalate", None),
        decision("sha256:aa", "n1", "2026-06-09T11:00:00Z", "allow", None),
    ])
    .success()
    .get_output()
    .stdout
    .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["groups"][0]["verdict"], "resolved");
    assert_eq!(report["groups"][0]["effective_decision"], "allow");
    assert_eq!(
        report["groups"][0]["effective_decided_at"],
        "2026-06-09T11:00:00Z"
    );
}

#[test]
fn equal_decided_at_without_ordering_is_ambiguous() {
    // The headline case: a tie with no ordering field must not be silently resolved.
    // Same call binding (same digest AND nonce), same decidedAt, no ordering field.
    run(&[
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "escalate", None),
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "allow", None),
    ])
    .code(2)
    .stdout(predicate::str::contains("ambiguous"))
    .stdout(predicate::str::contains("no explicit ordering field"));
}

#[test]
fn equal_decided_at_with_sequence_resolves() {
    let out = run(&[
        decision(
            "sha256:aa",
            "n1",
            "2026-06-09T10:00:00Z",
            "escalate",
            Some(1),
        ),
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "allow", Some(2)),
    ])
    .success()
    .get_output()
    .stdout
    .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["groups"][0]["verdict"], "resolved");
    assert_eq!(report["groups"][0]["effective_decision"], "allow");
}

#[test]
fn equal_decided_at_and_equal_sequence_is_ambiguous() {
    run(&[
        decision(
            "sha256:aa",
            "n1",
            "2026-06-09T10:00:00Z",
            "escalate",
            Some(5),
        ),
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "allow", Some(5)),
    ])
    .code(2)
    .stdout(predicate::str::contains("ambiguous"))
    .stdout(predicate::str::contains("equal sequence"));
}

#[test]
fn distinct_backlinks_each_resolve_independently() {
    let out = run(&[
        decision("sha256:aa", "n1", "2026-06-09T10:00:00Z", "allow", None),
        decision("sha256:bb", "n2", "2026-06-09T10:00:00Z", "block", None),
    ])
    .success()
    .get_output()
    .stdout
    .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["groups"].as_array().unwrap().len(), 2);
}
