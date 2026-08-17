//! `assay evidence diff` must qualify comparison scope on the output that invites over-read (#2445).
//!
//! The computed set difference is over retained verified `.net` / `.fs` / `.process` subjects.
//! Session-finding `extent` does not license those absences. This contract pins the CLI surface
//! only: one human sentence on stderr before any sets, and one additive JSON object. It does not
//! change `DiffReport`, the engine, verification, or exit codes.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

const SCOPE_SENTENCE: &str =
    "Comparison scope: retained verified events. Absence and completeness are not established.";

fn write_bundle(path: &Path, run_id: &str, include_config: bool) {
    let mut events = Vec::new();

    let mut started = EvidenceEvent::new(
        "assay.profile.started",
        "urn:assay:test",
        run_id,
        0,
        serde_json::json!({"name": "diff-scope"}),
    );
    started.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    events.push(started);

    let mut net = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        run_id,
        1,
        serde_json::json!({"host": "api.example.com"}),
    );
    net.time = Utc.timestamp_opt(1_700_000_001, 0).unwrap();
    net = net.with_subject("api.example.com:443");
    events.push(net);

    if include_config {
        let mut fs = EvidenceEvent::new(
            "assay.fs.access",
            "urn:assay:test",
            run_id,
            2,
            serde_json::json!({"path": "/etc/config"}),
        );
        fs.time = Utc.timestamp_opt(1_700_000_002, 0).unwrap();
        fs = fs.with_subject("/etc/config");
        events.push(fs);
    }

    let mut file = std::fs::File::create(path).expect("create bundle");
    let mut writer = BundleWriter::new(&mut file);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("finish bundle");
}

fn run_diff(baseline: &Path, candidate: &Path, format: &str) -> assert_cmd::assert::Assert {
    Command::cargo_bin("assay")
        .expect("assay binary")
        .args([
            "evidence",
            "diff",
            baseline.to_str().expect("utf8 baseline"),
            candidate.to_str().expect("utf8 candidate"),
            "--format",
            format,
        ])
        .assert()
}

fn scope_appears_before(haystack: &str, later: &str) {
    let scope_at = haystack
        .find(SCOPE_SENTENCE)
        .unwrap_or_else(|| panic!("stderr must contain the exact scope sentence:\n{haystack}"));
    let later_at = haystack
        .find(later)
        .unwrap_or_else(|| panic!("stderr must contain {later:?}:\n{haystack}"));
    assert!(
        scope_at < later_at,
        "scope sentence must appear before {later:?}"
    );
}

fn assert_no_extent_license(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            assert!(
                !map.contains_key("extent"),
                "{path} must not introduce an extent license key"
            );
            for (key, child) in map {
                assert_no_extent_license(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                assert_no_extent_license(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

#[test]
fn empty_human_diff_prints_scope_before_no_differences_found() {
    let dir = tempdir().expect("tempdir");
    let baseline = dir.path().join("baseline.tar.gz");
    let candidate = dir.path().join("candidate.tar.gz");
    write_bundle(&baseline, "run_base", true);
    write_bundle(&candidate, "run_cand", true);

    let output = run_diff(&baseline, &candidate, "human")
        .success()
        .get_output()
        .clone();
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    scope_appears_before(&stderr, "No differences found.");
}

#[test]
fn removed_config_human_diff_prints_scope_before_the_removed_path() {
    let dir = tempdir().expect("tempdir");
    let baseline = dir.path().join("baseline.tar.gz");
    let candidate = dir.path().join("candidate.tar.gz");
    write_bundle(&baseline, "run_base", true);
    write_bundle(&candidate, "run_cand", false);

    let output = run_diff(&baseline, &candidate, "human")
        .success()
        .get_output()
        .clone();
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("  - /etc/config"),
        "removed path must still be reported:\n{stderr}"
    );
    scope_appears_before(&stderr, "  - /etc/config");
}

#[test]
fn json_diff_publishes_comparison_scope_and_keeps_report_fields() {
    let dir = tempdir().expect("tempdir");
    let baseline = dir.path().join("baseline.tar.gz");
    let candidate = dir.path().join("candidate.tar.gz");
    write_bundle(&baseline, "run_base", true);
    write_bundle(&candidate, "run_cand", false);

    let output = run_diff(&baseline, &candidate, "json")
        .success()
        .get_output()
        .clone();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout utf8");
    let report: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");

    assert_eq!(
        report["comparison_scope"],
        serde_json::json!({
            "basis": "retained_verified_events",
            "absence_completeness": "not_established"
        })
    );
    assert!(
        report.get("baseline").is_some(),
        "existing baseline field must stay top-level"
    );
    assert!(
        report.get("candidate").is_some(),
        "existing candidate field must stay top-level"
    );
    assert!(
        report.get("filesystem").is_some(),
        "existing filesystem field must stay top-level"
    );
    assert_eq!(
        report["comparison_scope"]["absence_completeness"],
        "not_established"
    );
    assert_no_extent_license(&report, "$");
}
