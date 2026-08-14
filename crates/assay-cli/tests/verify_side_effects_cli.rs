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

#[test]
fn a_peer_set_from_a_different_run_cannot_refute() {
    // The invariant the shared run identity exists for. Peers from run A checked against run B's
    // coverage would let a well-covered run vouch for a blind one, so a mismatched pair refuses
    // rather than compares.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);

    let oh = dir.path().join("oh.json");
    fs::write(
        &oh,
        serde_json::to_string(&json!({
            "schema": "assay.runner.observation_health.v0",
            "run_id": "run-A",
            "kernel_layer": "complete", "ringbuf_drops": 0,
            "network_protocol_coverage": "connect_only", "cgroup_correlation": "clean",
        }))
        .unwrap(),
    )
    .unwrap();

    let peers = dir.path().join("peers.json");
    fs::write(
        &peers,
        serde_json::to_string(&json!({
            "schema": "assay.monitor.observed_peers.v0",
            "run_id": "run-B",
            "peers": [],
        }))
        .unwrap(),
    )
    .unwrap();

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--observation-health")
        .arg(&oh)
        .arg("--observed-peers")
        .arg(&peers)
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(
        report["calls"][0]["egress"]["outcome"],
        json!("no_coverage"),
        "a mismatched run pair must refuse, not refute"
    );
}

// ------------------------------------------------- the ceiling ladder, made observable in output

#[test]
fn the_ladder_ordering_is_visible_in_the_report_and_not_only_in_the_type_system() {
    // The gate has always computed a rung and this report used to drop it, so `asserted` and
    // `verified` reached a consumer as a bare verdict and the ordering
    // `producer_reported < ... < independently_confirmed` decided nothing anyone could read.
    //
    // This test fails two ways on purpose. It fails if the rung stops being carried, and it fails if
    // someone flattens the ladder so an independently produced record grades no higher than the
    // provider's own word — the mutation `rge-bench`'s liveness check kills on the same axis.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let asserted = run(&bundle, None);
    let verified = run(&bundle, Some(&import));

    assert_eq!(
        asserted["calls"][0]["occurrence_ceiling"],
        json!("asserted"),
        "a provider's own word caps at `asserted`, whatever else is true of the run"
    );
    assert_eq!(
        verified["calls"][0]["occurrence_ceiling"],
        json!("independently_confirmed"),
        "a bound record from the system that would know is what the top rung is for"
    );
    assert_ne!(
        asserted["calls"][0]["occurrence_ceiling"], verified["calls"][0]["occurrence_ceiling"],
        "if these ever agree the ladder has been flattened and buys nothing"
    );
}

/// Two asserting calls in one bundle, only one of which an imported record can bind.
///
/// A single-call bundle cannot tell a fold from a maximum — with one row they are the same number —
/// so a test written against the committed one-call surface would pass under either rule and prove
/// nothing. This builds the input that can discriminate: the second call targets a different repo,
/// so the deploy-key record binds the first and leaves the second at the provider's own word.
fn bundle_with_one_bindable_and_one_unbindable_call(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let mut first = surface["observed_tool_decisions"][0].clone();
    first["response"]["side_effect"] = json!({ "asserted": true, "level": "asserted" });
    first["response"]["side_effect_verified"] = json!(false);

    let mut second = first.clone();
    second["tool"]["name"] = json!("github.add_deploy_key_other");
    second["action"]["target"]["repo"] = json!("some-other-repo");

    surface["observed_tool_decisions"] = json!([first, second]);

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-side-effects-two",
        0,
        surface,
    );
    event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn the_run_level_ceiling_is_the_weakest_rung_and_never_the_strongest() {
    // A fold, not a maximum. One corroborated call does not raise what the run as a whole supports,
    // and a reader who takes the best row home has learnt the wrong thing.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("two.tar.gz");
    bundle_with_one_bindable_and_one_unbindable_call(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));
    let rows: Vec<&Value> = report["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["asserted"] == json!(true))
        .collect();
    assert_eq!(rows.len(), 2, "the discriminating input must survive");

    let strongest = rows
        .iter()
        .map(|c| &c["occurrence_ceiling"])
        .max_by_key(|c| ladder_index(c))
        .unwrap();
    let weakest = rows
        .iter()
        .map(|c| &c["occurrence_ceiling"])
        .min_by_key(|c| ladder_index(c))
        .unwrap();
    assert_ne!(
        strongest, weakest,
        "without two different rungs this test cannot tell a fold from a maximum"
    );

    assert_eq!(
        report["weakest_occurrence_ceiling"],
        json!({ "state": "rung", "ceiling": weakest }),
        "the run-level rung must be the weakest of its asserting calls"
    );
}

#[test]
fn a_refutation_takes_the_rung_with_it_rather_than_leaving_a_strong_number_behind() {
    // `Blocked` is not the bottom rung, it is the statement that no rung applies. Leaving
    // `independently_confirmed` next to a blocked claim would let a consumer read the number and
    // drop the verdict, which is precisely what the refutation exists to stop.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let oh = health(dir.path(), "connect_only", "clean");

    let report = run_with_health(&bundle, Some(&import), &oh);

    assert_eq!(report["calls"][0]["occurrence_claim"], json!("blocked"));
    assert_eq!(
        report["calls"][0]["occurrence_ceiling"],
        Value::Null,
        "a blocked occurrence must not advertise a rung"
    );
    assert_eq!(
        report["weakest_occurrence_ceiling"],
        json!({ "state": "blocked" }),
        "and one blocked asserting call collapses the run-level answer, rather than lowering it"
    );
}

/// A bundle whose only observed call asserted no side effect.
fn bundle_with_no_asserted_side_effect(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let decision = &mut surface["observed_tool_decisions"][0];
    decision["response"]["side_effect"] = json!({ "asserted": false, "level": "asserted" });
    decision["response"]["side_effect_asserted"] = json!(false);
    decision["response"]["side_effect_verified"] = json!(false);

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-side-effects-none",
        0,
        surface,
    );
    event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn a_run_that_claimed_nothing_is_distinguishable_from_a_run_that_was_contradicted() {
    // The finding an adversarial review caught, pinned so it cannot come back. The first version of
    // the run-level field was an `Option` skipped when empty, so it was ABSENT both when a refutation
    // collapsed the run and when no call asserted anything at all. A consumer could not tell "we
    // watched and the evidence was contradicted" from "nothing here ever claimed an effect" — which
    // is the occurrence-versus-absence conflation this whole command exists to prevent, reintroduced
    // by the field meant to carry the rule.
    let dir = tempdir().unwrap();

    let quiet = dir.path().join("quiet.tar.gz");
    bundle_with_no_asserted_side_effect(&quiet);
    let quiet_report = run(&quiet, None);

    let contradicted = dir.path().join("contradicted.tar.gz");
    bundle_with_asserted_decision(&contradicted);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let oh = health(dir.path(), "connect_only", "clean");
    let contradicted_report = run_with_health(&contradicted, Some(&import), &oh);

    assert_eq!(
        quiet_report["weakest_occurrence_ceiling"],
        json!({ "state": "nothing_claimed" })
    );
    assert_eq!(
        contradicted_report["weakest_occurrence_ceiling"],
        json!({ "state": "blocked" })
    );
    assert_ne!(
        quiet_report["weakest_occurrence_ceiling"],
        contradicted_report["weakest_occurrence_ceiling"],
        "silence and contradiction must not serialize to the same thing"
    );

    // And the row-level half: a call that asserted nothing carries no rung, because a ladder
    // position grades a claim and this call did not make one.
    assert_eq!(quiet_report["calls"][0]["asserted"], json!(false));
    assert_eq!(quiet_report["calls"][0]["occurrence_ceiling"], Value::Null);
}

/// Position on the published ladder, defined here rather than imported so the test does not agree
/// with the implementation by construction. A rung the binary emits that this list does not know is
/// a hard failure: it means the vocabulary grew and this test stopped covering it.
///
/// `Null` gets its own arm rather than falling into the panic. It is a legitimate value — a blocked
/// or non-asserting row carries no rung — and reporting it as an unknown rung would misdiagnose an
/// ordinary state as a vocabulary change.
fn ladder_index(v: &Value) -> usize {
    match v.as_str() {
        Some("asserted") => 0,
        Some("asserted_signed") => 1,
        Some("observed_at_receiver") => 2,
        Some("observed_in_path") => 3,
        Some("independently_confirmed") => 4,
        None if v.is_null() => panic!("no rung on this row; the caller must filter before ranking"),
        other => panic!("unknown ceiling rung in report: {other:?}"),
    }
}
