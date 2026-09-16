//! `assay evidence diff` must qualify comparison scope on the output that invites over-read (#2445)
//! and must not report a clean result when the retained events differ outside the subject
//! projections (#3037).
//!
//! The subject sets are over retained verified `.net` / `.fs` / `.process` subjects; the retained
//! events themselves are compared by verified content id, with `run_root` as the whole-record
//! witness. Session-finding `extent` does not license absences. This contract pins the CLI surface:
//! one human sentence on stderr before any sets, the equal / differs line, and additive JSON. It
//! does not change verification or exit codes.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

const SCOPE_SENTENCE: &str =
    "Comparison scope: retained verified events. Absence and completeness are not established.";
const EQUAL_LINE: &str = "No differences in retained verified events: run_root equal.";
const DIFFERS_PREFIX: &str = "Retained verified events differ: run_root differs;";

fn write_bundle(path: &Path, run_id: &str, include_config: bool) {
    write_bundle_with_decision(path, run_id, include_config, None);
}

/// `decision` adds an `assay.policy.decision` event whose payload no subject projection reads.
fn write_bundle_with_decision(
    path: &Path,
    run_id: &str,
    include_config: bool,
    decision: Option<&str>,
) {
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

    if let Some(decision) = decision {
        let mut policy = EvidenceEvent::new(
            "assay.policy.decision",
            "urn:assay:test",
            run_id,
            events.len() as u64,
            serde_json::json!({"tool": "write_file", "decision": decision}),
        );
        policy.time = Utc.timestamp_opt(1_700_000_003, 0).unwrap();
        events.push(policy);
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
    scope_appears_before(&stderr, EQUAL_LINE);
    assert!(
        !stderr.contains("No differences found."),
        "the unqualified clean line must not be printed:\n{stderr}"
    );
}

fn write_decision_pair(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let baseline = dir.join("baseline.tar.gz");
    let candidate = dir.join("candidate.tar.gz");
    write_bundle_with_decision(&baseline, "run_base", true, Some("allow"));
    write_bundle_with_decision(&candidate, "run_cand", true, Some("deny"));
    (baseline, candidate)
}

fn assert_differs_surfaced(stderr: &str, run_root_a: &str, run_root_b: &str) {
    assert!(
        !stderr.contains("No differences found.") && !stderr.contains(EQUAL_LINE),
        "equal projections with differing run_root must not read as clean:\n{stderr}"
    );
    assert!(
        stderr.contains(run_root_a) && stderr.contains(run_root_b),
        "both run_roots must be printed:\n{stderr}"
    );
    assert!(
        stderr.contains(DIFFERS_PREFIX),
        "the differs line must be printed:\n{stderr}"
    );
    assert!(
        stderr.contains("assay.policy.decision"),
        "the differing event must be named by type:\n{stderr}"
    );
    scope_appears_before(stderr, DIFFERS_PREFIX);
}

fn run_roots(baseline: &Path, candidate: &Path) -> (String, String) {
    let output = run_diff(baseline, candidate, "json")
        .success()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout).expect("json");
    let a = report["baseline"]["run_root"].as_str().expect("run_root a");
    let b = report["candidate"]["run_root"]
        .as_str()
        .expect("run_root b");
    assert_ne!(a, b, "fixture must differ in run_root");
    (a.to_string(), b.to_string())
}

#[test]
fn equal_projections_differing_run_root_human_is_not_clean() {
    let dir = tempdir().expect("tempdir");
    let (baseline, candidate) = write_decision_pair(dir.path());
    let (root_a, root_b) = run_roots(&baseline, &candidate);

    let output = run_diff(&baseline, &candidate, "human")
        .success()
        .get_output()
        .clone();
    assert_eq!(
        output.status.code(),
        Some(0),
        "exit code contract is unchanged"
    );
    assert_differs_surfaced(&String::from_utf8_lossy(&output.stderr), &root_a, &root_b);
}

#[test]
fn equal_projections_differing_run_root_json_carries_the_comparison() {
    let dir = tempdir().expect("tempdir");
    let (baseline, candidate) = write_decision_pair(dir.path());

    let output = run_diff(&baseline, &candidate, "json")
        .success()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout).expect("json");
    let events = &report["retained_events"];
    assert_eq!(events["run_root_equal"], Value::Bool(false));
    assert_eq!(events["added"].as_array().map(Vec::len), Some(1));
    assert_eq!(events["removed"].as_array().map(Vec::len), Some(1));
    assert_eq!(events["added"][0]["type"], "assay.policy.decision");
    assert_eq!(events["removed"][0]["type"], "assay.policy.decision");
    assert_eq!(
        report["comparison_scope"]["basis"],
        "retained_verified_events"
    );
    assert_no_extent_license(&report, "$");
}

#[test]
fn equal_projections_differing_run_root_baseline_dir_is_not_clean() {
    let dir = tempdir().expect("tempdir");
    let baselines = dir.path().join("baselines");
    std::fs::create_dir_all(&baselines).expect("baseline dir");
    write_bundle_with_decision(
        &baselines.join("base.tar.gz"),
        "run_base",
        true,
        Some("allow"),
    );
    let candidate = dir.path().join("candidate.tar.gz");
    write_bundle_with_decision(&candidate, "run_cand", true, Some("deny"));
    let (root_a, root_b) = run_roots(&baselines.join("base.tar.gz"), &candidate);

    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args([
            "evidence",
            "diff",
            candidate.to_str().expect("utf8 candidate"),
            "--baseline-dir",
            baselines.to_str().expect("utf8 dir"),
            "--key",
            "base",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_differs_surfaced(&String::from_utf8_lossy(&output.stderr), &root_a, &root_b);
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
