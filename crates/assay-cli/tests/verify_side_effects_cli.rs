//! Eb.4 end to end: the CLI promotes only against a binding audit record, and reports the rest.
//!
//! The unit tests prove the verifier is correct over values. This proves the *command* is wired to
//! it: bundle in, audit import in, promotion out. A green unit suite next to an unwired command is
//! exactly the Ea situation this slice exists to end.

use assay_evidence::{BundleWriter, EvidenceEvent};
use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use std::fs;
use tempfile::tempdir;

const DECISION_EVENT_TYPE: &str = "assay.tool_decision_surface.v0";

fn fixture(name: &str) -> Value {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assay-mcp-server/tests/fixtures/side_effect")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
}

/// A bundle carrying the committed `verified.json` decision surface, with the side-effect block
/// reset to `asserted` so promotion has to be earned rather than read back out of the fixture.
fn bundle_with_asserted_decision(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let decision = &mut surface["observed_tool_decisions"][0];
    decision["response"]["side_effect"] = json!({ "asserted": true, "level": "asserted" });
    decision["response"]["side_effect_verified"] = json!(false);

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-side-effects",
        0,
        surface,
    );
    event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

fn import_dir(dir: &std::path::Path, record_fixture: &str) -> std::path::PathBuf {
    let out = dir.join("audit");
    fs::create_dir_all(&out).unwrap();
    fs::write(
        out.join("record.json"),
        serde_json::to_string(&fixture(record_fixture)).unwrap(),
    )
    .unwrap();
    out
}

fn run(bundle: &std::path::Path, import: Option<&std::path::Path>) -> Value {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("verify-side-effects")
        .arg(bundle)
        .arg("--format")
        .arg("json");
    if let Some(dir) = import {
        cmd.arg("--audit-import").arg(dir);
    }
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).expect("json report")
}

#[test]
fn a_binding_audit_record_promotes_the_call_to_verified() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(1));
    assert_eq!(report["calls"][0]["level"], json!("verified"));
    assert_eq!(report["audit_records_unmatched"], json!(0));
    assert!(report["calls"][0]["subject_digest"].is_string());
}

#[test]
fn a_mismatched_record_leaves_the_call_asserted_and_says_why() {
    // The rule the ladder exists for: not promoted, and not silently dropped either.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_mismatch.json");

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(0));
    assert_eq!(report["calls"][0]["level"], json!("asserted"));
    assert_eq!(
        report["calls"][0]["binding"]["outcome"],
        json!("binds_different_call"),
        "a rejected record must be reported with its reason"
    );
    assert_eq!(
        report["audit_records_unmatched"],
        json!(1),
        "an imported record that bound to nothing is counted, not discarded"
    );
}

#[test]
fn no_import_leaves_everything_asserted_without_failing() {
    // The ordinary case. Absence of an audit export is not an error, and must not read as one.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);

    let report = run(&bundle, None);

    assert_eq!(report["promoted"], json!(0));
    assert_eq!(report["calls"][0]["level"], json!("asserted"));
    assert_eq!(report["audit_records_imported"], json!(0));
    assert!(
        report["calls"][0]["binding"].is_null(),
        "nothing was considered"
    );
}

#[test]
fn the_report_declares_what_it_does_not_claim() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));
    let claims: Vec<&str> = report["claims_not_made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Promotion to `verified` must never be readable as "Assay asked the provider".
    assert!(claims.contains(&"provider_query"));
    assert!(claims.contains(&"audit_record_authenticity_beyond_its_own_signature"));
}

#[test]
fn the_table_render_path_works_and_names_the_rejection() {
    // The other tests all use --format json, which would leave the human-facing render untested and
    // able to panic in the field on a `serde_json::to_string` of the binding enum.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_mismatch.json");

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--audit-import")
        .arg(&import)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();

    assert!(text.contains("level=asserted"), "{text}");
    assert!(
        text.contains("not promoted"),
        "a rejection must be visible, not only in JSON"
    );
    assert!(text.contains("binds_different_call"), "{text}");
    assert!(text.contains("promoted to verified: 0"), "{text}");
}

// ---------------------------------------------------------------- Eb.5: the gate binds

#[test]
fn an_asserted_side_effect_cannot_carry_an_occurrence_claim() {
    // The point of the whole ladder. The provider said success; that is the provider's word, and it
    // must not license "this effect happened" downstream.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);

    let report = run(&bundle, None);
    assert_eq!(report["calls"][0]["level"], json!("asserted"));
    assert_ne!(
        report["calls"][0]["occurrence_claim"],
        json!("allowed"),
        "producer-reported evidence must not license an occurrence claim"
    );
}

#[test]
fn a_verified_side_effect_can_carry_an_occurrence_claim() {
    // And the converse, or the ladder buys nothing: an independently produced record that binds is
    // exactly the evidence an occurrence claim needs.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));
    assert_eq!(report["calls"][0]["level"], json!("verified"));
    assert_eq!(report["calls"][0]["occurrence_claim"], json!("allowed"));
}

#[test]
fn no_level_ever_supports_an_absence_claim() {
    // The asymmetry the runner gate already encodes, inherited here rather than restated: seeing a
    // write happen says nothing about what else did or did not. Even `verified` cannot say "and
    // nothing else occurred", because an audit record for one call is not coverage of a dimension.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    for report in [run(&bundle, None), run(&bundle, Some(&import))] {
        assert_ne!(
            report["calls"][0]["bounded_negative_claim"],
            json!("allowed"),
            "no side-effect level is coverage of a dimension"
        );
    }
}

// ---------------------------------------------------------------- Ec: refutation from below

fn health(dir: &std::path::Path, coverage: &str, correlation: &str) -> std::path::PathBuf {
    let p = dir.join("observation_health.json");
    fs::write(
        &p,
        serde_json::to_string(&json!({
            "schema": "assay.runner.observation_health.v0",
            "kernel_layer": "complete",
            "ringbuf_drops": 0,
            "network_protocol_coverage": coverage,
            "cgroup_correlation": correlation,
        }))
        .unwrap(),
    )
    .unwrap();
    p
}

fn run_with_health(
    bundle: &std::path::Path,
    import: Option<&std::path::Path>,
    oh: &std::path::Path,
) -> Value {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("verify-side-effects")
        .arg(bundle)
        .arg("--observation-health")
        .arg(oh)
        .arg("--format")
        .arg("json");
    if let Some(d) = import {
        cmd.arg("--audit-import").arg(d);
    }
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn a_watching_observer_that_saw_nothing_refutes_the_egress() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let oh = health(dir.path(), "connect_only", "clean");

    let report = run_with_health(&bundle, None, &oh);
    assert_eq!(report["calls"][0]["egress"]["outcome"], json!("refuted"));
    assert_eq!(
        report["calls"][0]["egress"]["watched_surface"],
        json!("cgroup_sock_addr:connect4"),
        "a refutation names the surface it watched, not the world"
    );
}

#[test]
fn a_blind_observer_does_not_refute_anything() {
    // The property Ec exists for. Same empty peer set, no coverage: silence stays silence.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let oh = health(dir.path(), "absent", "clean");

    let report = run_with_health(&bundle, None, &oh);
    assert_eq!(
        report["calls"][0]["egress"]["outcome"],
        json!("no_coverage")
    );
}

#[test]
fn a_refutation_blocks_the_occurrence_claim_even_when_an_audit_record_verified_it() {
    // The conflict case, and the sharpest thing this command does. An imported audit record says the
    // call happened; a watching kernel observer says nothing left the cgroup. They disagree, so the
    // occurrence claim is blocked and BOTH are shown. Silently preferring the higher rung would make
    // the observer decorative.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let oh = health(dir.path(), "connect_only", "clean");

    let report = run_with_health(&bundle, Some(&import), &oh);
    let call = &report["calls"][0];

    assert_eq!(
        call["level"],
        json!("verified"),
        "the audit record still bound"
    );
    assert_eq!(
        call["egress"]["outcome"],
        json!("refuted"),
        "and the kernel still disagrees"
    );
    assert_eq!(
        call["occurrence_claim"],
        json!("blocked"),
        "a contradicted occurrence must not be claimable from either side"
    );
}

#[test]
fn partial_correlation_cannot_overturn_a_verified_record() {
    // The inverse guard: a probe gap must not be able to refute real corroboration.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let oh = health(dir.path(), "connect_only", "partial");

    let report = run_with_health(&bundle, Some(&import), &oh);
    assert_eq!(
        report["calls"][0]["egress"]["outcome"],
        json!("coverage_degraded")
    );
    assert_eq!(
        report["calls"][0]["occurrence_claim"],
        json!("allowed"),
        "a degraded observer must not overturn an audit record that bound"
    );
}
