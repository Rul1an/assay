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
    assert_eq!(
        report["groups"][0]["reason_code"],
        "supersession_resolved_single"
    );
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
    assert_eq!(
        report["groups"][0]["reason_code"],
        "supersession_resolved_latest_decided_at"
    );
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
    .stdout(predicate::str::contains(
        "supersession_ambiguous_missing_sequence",
    ));
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
    assert_eq!(
        report["groups"][0]["reason_code"],
        "supersession_resolved_sequence"
    );
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
    .stdout(predicate::str::contains(
        "supersession_ambiguous_duplicate_sequence",
    ));
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

/// The table quotes each group's call binding, which comes from the records as received. Whatever a
/// record holds there, stdout has no control byte except the newline that ends a line, no C1
/// control and no bidirectional control, and a record cannot add a line of its own. `--format json`
/// is data and must carry the binding unchanged.
#[test]
fn the_table_cannot_write_to_the_terminal_through_a_call_binding() {
    let hostile_digest =
        "sha256:aa\u{1b}]52;c;cHduZWQ=\u{7}\u{1b}[31mRED\u{9b}31m\u{202e}\r\nOK:    yes";
    let hostile_nonce = "n1\u{1b}]0;TITLE\u{7}\nClaims not made: none";
    let records = [decision(
        hostile_digest,
        hostile_nonce,
        "2026-06-09T10:00:00Z",
        "allow",
        None,
    )];
    let dir = tempdir().unwrap();
    let path = dir.path().join("decisions.json");
    fs::write(
        &path,
        serde_json::to_string(&Value::Array(records.to_vec())).unwrap(),
    )
    .unwrap();
    let table = |format: &str| {
        Command::cargo_bin("assay")
            .unwrap()
            .args([
                "evidence",
                "verify-mcp-supersession",
                "--decisions",
                path.to_str().unwrap(),
                "--format",
                format,
            ])
            .output()
            .unwrap()
    };

    let out = table("table");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout.clone()).expect("the table is UTF-8");
    assert!(
        text.contains("attestationDigest=sha256:aa"),
        "the binding did not reach the table:\n{text}"
    );
    for (index, &byte) in out.stdout.iter().enumerate() {
        assert!(
            (byte >= 0x20 && byte != 0x7f) || byte == b'\n',
            "stdout byte {index} is the control byte {byte:#04x}:\n{text}"
        );
    }
    for c in text.chars() {
        assert!(
            !('\u{80}'..='\u{9f}').contains(&c)
                && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
            "U+{:04X} reached the terminal:\n{text}",
            c as u32
        );
    }
    assert_eq!(
        text.lines().filter(|l| l.starts_with("OK:")).count(),
        1,
        "a record wrote an OK line of its own:\n{text}"
    );
    assert_eq!(
        text.lines()
            .filter(|l| l.starts_with("Claims not made:"))
            .count(),
        1,
        "a record wrote a Claims line of its own:\n{text}"
    );

    let json: Value = serde_json::from_slice(&table("json").stdout).unwrap();
    assert_eq!(
        json["groups"][0]["backlink_key"],
        format!("attestationDigest={hostile_digest};attestationNonce={hostile_nonce}"),
        "the JSON form changed the binding"
    );
}
