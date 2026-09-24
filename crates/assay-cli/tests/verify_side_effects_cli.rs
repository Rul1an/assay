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
    assert!(
        text.contains("occurrence=Degraded reason=self_reported_only"),
        "{text}"
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

#[test]
fn a_qualified_claim_names_its_reason_in_the_report() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);

    let report = run(&bundle, None);
    let call = &report["calls"][0];
    assert_eq!(call["occurrence_claim"], json!("degraded"));
    assert_eq!(
        call["occurrence_reason"],
        json!({
            "origin": "claim_gate",
            "gap": "self_reported_only",
            "rule": "self_reported_degrades_positive_claim",
        })
    );
    assert_eq!(
        call["bounded_negative_reason"],
        json!({
            "origin": "claim_gate",
            "gap": "self_reported_only",
            "rule": "self_reported_blocks_completeness_claim",
        })
    );
}

#[test]
fn a_refuted_occurrence_names_the_refutation_and_not_a_gate_rule() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let oh = health(dir.path(), "connect_only", "clean");

    let report = run_with_health(&bundle, Some(&import), &oh);
    let call = &report["calls"][0];
    assert_eq!(call["occurrence_claim"], json!("blocked"));
    assert_eq!(
        call["occurrence_reason"],
        json!({
            "origin": "observer_refutation",
        })
    );
    assert_ne!(
        call["occurrence_reason"]["rule"],
        json!("partial_coverage_allows_positive_claim")
    );
}

#[test]
fn a_reason_is_present_exactly_when_the_verdict_is_not_allowed() {
    let dir = tempdir().unwrap();

    let one_call_bundle = dir.path().join("one.tar.gz");
    bundle_with_asserted_decision(&one_call_bundle);
    let one_call_report = run(&one_call_bundle, None);

    let two_call_bundle = dir.path().join("two.tar.gz");
    bundle_with_one_bindable_and_one_unbindable_call(&two_call_bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");
    let two_call_report = run(&two_call_bundle, Some(&import));

    let no_assert_bundle = dir.path().join("none.tar.gz");
    bundle_with_no_asserted_side_effect(&no_assert_bundle);
    let no_assert_report = run(&no_assert_bundle, None);

    for report in [one_call_report, two_call_report, no_assert_report] {
        for call in report["calls"].as_array().expect("report has calls array") {
            let occurrence_allowed = call["occurrence_claim"] == json!("allowed");
            let occurrence_reason_present = !call["occurrence_reason"].is_null();
            assert_eq!(
                occurrence_reason_present, !occurrence_allowed,
                "occurrence_reason must be present iff occurrence_claim is not allowed"
            );

            let bounded_negative_allowed = call["bounded_negative_claim"] == json!("allowed");
            let bounded_negative_reason_present = !call["bounded_negative_reason"].is_null();
            assert_eq!(
                bounded_negative_reason_present, !bounded_negative_allowed,
                "bounded_negative_reason must be present iff bounded_negative_claim is not allowed"
            );
        }
    }
}

#[test]
fn an_allowed_claim_carries_no_reason() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_asserted_decision(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));
    let call = &report["calls"][0];
    assert_eq!(call["occurrence_claim"], json!("allowed"));
    assert_eq!(
        call["occurrence_reason"],
        Value::Null,
        "allowed rows stay reasonless"
    );
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

// ------------------------------------------------- Blocker 1: fail closed on malformed input

/// A bundle carrying an event that is NOT a decision surface (wrong type).
fn bundle_with_no_decision_events(path: &std::path::Path) {
    let event = EvidenceEvent::new(
        "assay.some_other_event.v0",
        "urn:assay:test:side-effects-cli",
        "run-no-decisions",
        0,
        json!({"irrelevant": true}),
    );
    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn a_bundle_with_no_decision_events_fails_rather_than_reporting_nothing_claimed() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("empty.tar.gz");
    bundle_with_no_decision_events(&bundle);

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(out).unwrap();
    assert!(
        stderr.contains("no") && stderr.contains(DECISION_EVENT_TYPE),
        "the error must name the missing event type: {stderr}"
    );
}

/// A bundle whose decision event has `observed_tool_decisions` as a string instead of an array.
fn bundle_with_non_array_decisions(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    surface["observed_tool_decisions"] = json!("not_an_array");

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-bad-decisions",
        0,
        surface,
    );
    event.time = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn a_non_array_observed_tool_decisions_fails_rather_than_silently_skipping() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("bad.tar.gz");
    bundle_with_non_array_decisions(&bundle);

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(out).unwrap();
    assert!(
        stderr.contains("not an array"),
        "the error must explain what went wrong: {stderr}"
    );
}

/// A bundle where response.side_effect_asserted is a string instead of a bool.
fn bundle_with_non_bool_asserted(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let decision = &mut surface["observed_tool_decisions"][0];
    decision["response"]["side_effect_asserted"] = json!("yes");

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-bad-asserted",
        0,
        surface,
    );
    event.time = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn a_non_bool_side_effect_asserted_fails_rather_than_defaulting_to_false() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("bad.tar.gz");
    bundle_with_non_bool_asserted(&bundle);

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(out).unwrap();
    assert!(
        stderr.contains("not a boolean"),
        "the error must explain what went wrong: {stderr}"
    );
}

/// A bundle where response.side_effect_asserted is missing entirely.
fn bundle_with_missing_asserted(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let decision = &mut surface["observed_tool_decisions"][0];
    if let Some(resp) = decision.get_mut("response").and_then(Value::as_object_mut) {
        resp.remove("side_effect_asserted");
    }

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-missing-asserted",
        0,
        surface,
    );
    event.time = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn a_missing_side_effect_asserted_fails_rather_than_defaulting_to_false() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("bad.tar.gz");
    bundle_with_missing_asserted(&bundle);

    let out = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify-side-effects")
        .arg(&bundle)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(out).unwrap();
    assert!(
        stderr.contains("side_effect_asserted"),
        "the error must name the missing field: {stderr}"
    );
}

// ------------------------------------------------- Blocker 2: one-to-one audit allocation

/// Two identical asserting calls in one bundle (same tool, same target).
fn bundle_with_two_identical_asserting_calls(path: &std::path::Path) {
    let mut surface = fixture("verified.json");
    let mut call = surface["observed_tool_decisions"][0].clone();
    call["response"]["side_effect"] = json!({ "asserted": true, "level": "asserted" });
    call["response"]["side_effect_verified"] = json!(false);
    // Two identical calls targeting the same resource.
    surface["observed_tool_decisions"] = json!([call.clone(), call]);

    let mut event = EvidenceEvent::new(
        DECISION_EVENT_TYPE,
        "urn:assay:test:side-effects-cli",
        "run-two-identical",
        0,
        surface,
    );
    event.time = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    let file = fs::File::create(path).unwrap();
    let mut writer = BundleWriter::new(file);
    writer.add_event(event);
    writer.finish().unwrap();
}

#[test]
fn one_audit_record_cannot_vouch_for_two_identical_calls() {
    // Two otherwise identical asserting calls plus one audit record that binds their shape. Reusing
    // the record for both would overcount corroboration: two receipts for one piece of paper. An
    // earlier version promoted exactly one of the two, which avoided the overcount but picked the
    // call by listing order: the record carries no call identity and cannot say which call it
    // belongs to. Neither is promoted now, both say why, and the record is counted as held.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("two.tar.gz");
    bundle_with_two_identical_asserting_calls(&bundle);
    let import = import_dir(dir.path(), "audit_record_github_deploy_key.json");

    let report = run(&bundle, Some(&import));
    let calls = report["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 2, "both calls must appear");

    for call in calls {
        assert_eq!(
            call["level"],
            json!("asserted"),
            "one record cannot promote either of two calls it cannot tell apart"
        );
        assert_eq!(call["allocation"]["outcome"], json!("ambiguous"));
    }
    assert_eq!(report["promoted"], json!(0));
    assert_eq!(report["audit_records_ambiguous"], json!(1));

    // The run-level ceiling is the weakest rung (asserted), not the strongest.
    assert_eq!(
        report["weakest_occurrence_ceiling"],
        json!({ "state": "rung", "ceiling": "asserted" }),
        "no call was promoted, so the run supports no more than asserted"
    );
}

// ------------------------------------------------- ladder helpers

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

// --- Allocation among calls of the same action shape, and denial conflicts ---------------------

/// A bundle whose decision surface carries `copies` identical asserting calls, each with the given
/// decision effect and enforcement flag.
fn bundle_with_same_shape_calls(
    path: &std::path::Path,
    copies: usize,
    effect: &str,
    enforced: bool,
) {
    let mut surface = fixture("verified.json");
    let mut decision = surface["observed_tool_decisions"][0].clone();
    decision["response"]["side_effect"] = json!({ "asserted": true, "level": "asserted" });
    decision["response"]["side_effect_verified"] = json!(false);
    decision["decision"]["effect"] = json!(effect);
    decision["decision"]["enforced"] = json!(enforced);
    surface["observed_tool_decisions"] = Value::Array(vec![decision; copies]);

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

/// Import directory holding the binding record under each `(file name, record_id)` pair.
fn import_records(dir: &std::path::Path, records: &[(&str, &str)]) -> std::path::PathBuf {
    let out = dir.join("audit");
    fs::create_dir_all(&out).unwrap();
    for (name, record_id) in records {
        let mut record = fixture("audit_record_github_deploy_key.json");
        record["record_id"] = json!(record_id);
        fs::write(out.join(name), serde_json::to_string(&record).unwrap()).unwrap();
    }
    out
}

#[test]
fn fewer_records_than_same_shape_calls_promotes_none_of_them() {
    // Two calls with one action shape, one audit record for that shape. The record cannot say which
    // call it belongs to, so naming one would be a verdict decided by ordering.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 2, "allow", true);
    let import = import_records(dir.path(), &[("record.json", "entry-1")]);

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(0));
    for call in report["calls"].as_array().unwrap() {
        assert_eq!(call["level"], json!("asserted"));
        assert_eq!(call["allocation"]["outcome"], json!("ambiguous"));
        assert_eq!(call["allocation"]["calls"], json!(2));
        assert_eq!(call["allocation"]["records"], json!(1));
    }
    assert_eq!(report["audit_records_ambiguous"], json!(1));
    assert_eq!(
        report["audit_records_unmatched"],
        json!(0),
        "a record held back as ambiguous did bind a call shape; it is not unmatched"
    );
}

#[test]
fn enough_distinct_records_promote_every_same_shape_call_and_say_the_binding_is_by_shape() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 2, "allow", true);
    let import = import_records(dir.path(), &[("b.json", "entry-2"), ("a.json", "entry-1")]);

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(2));
    for call in report["calls"].as_array().unwrap() {
        assert_eq!(call["level"], json!("verified"));
        assert_eq!(call["allocation"]["outcome"], json!("shape_bound"));
    }
    assert_eq!(report["audit_records_ambiguous"], json!(0));
}

#[test]
fn the_same_record_delivered_twice_corroborates_one_call_not_two() {
    // Two files with byte-identical content are one audit entry delivered twice, not two entries.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 2, "allow", true);
    let import = import_records(dir.path(), &[("a.json", "entry-1"), ("b.json", "entry-1")]);

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(0));
    assert_eq!(report["calls"][0]["allocation"]["records"], json!(1));
    assert_eq!(report["audit_records_duplicate"], json!(1));
}

#[test]
fn the_same_record_with_a_number_spelled_differently_is_still_one_record() {
    // `1` and `1.0` are one JSON number; a re-serialized export must not become two entries.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 2, "allow", true);
    let out = dir.path().join("audit");
    fs::create_dir_all(&out).unwrap();
    let mut record = fixture("audit_record_github_deploy_key.json");
    record["seq"] = json!(1);
    fs::write(out.join("a.json"), serde_json::to_string(&record).unwrap()).unwrap();
    let text = serde_json::to_string(&record)
        .unwrap()
        .replace("\"seq\":1", "\"seq\":1.0");
    assert!(
        text.contains("\"seq\":1.0"),
        "the fixture must carry the alternate spelling"
    );
    fs::write(out.join("b.json"), text).unwrap();

    let report = run(&bundle, Some(&out));

    assert_eq!(report["audit_records_duplicate"], json!(1));
    assert_eq!(
        report["promoted"],
        json!(0),
        "one entry cannot promote two calls"
    );
}

#[test]
fn records_beyond_the_calls_of_their_shape_stay_unmatched() {
    // One call, two distinct records of its shape: the provider logged more effects than the bundle
    // observed calls. The call is promoted and the extra record is reported as unmatched, not hidden.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "allow", true);
    let import = import_records(dir.path(), &[("a.json", "entry-1"), ("b.json", "entry-2")]);

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(1));
    assert_eq!(report["audit_records_unmatched"], json!(1));
    assert_eq!(report["audit_records_ambiguous"], json!(0));
}

#[test]
fn a_single_call_and_its_record_is_call_bound_as_before() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "allow", true);
    let import = import_records(dir.path(), &[("record.json", "entry-1")]);

    let report = run(&bundle, Some(&import));

    assert_eq!(report["promoted"], json!(1));
    assert_eq!(report["calls"][0]["level"], json!("verified"));
    assert!(
        report["calls"][0]["allocation"].is_null(),
        "a shape with one call leaves nothing to allocate"
    );
}

#[test]
fn an_executed_call_under_a_denial_is_reported_as_a_conflict_and_not_dropped() {
    // The decision on the call says deny and enforced, the response asserts the effect, and an audit
    // record corroborates it. The records disagree; the disagreement is the finding. The execution
    // keeps its level: dropping or demoting it would hide the effect the denial failed to stop.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "deny", true);
    let import = import_records(dir.path(), &[("record.json", "entry-1")]);

    let report = run(&bundle, Some(&import));

    let call = &report["calls"][0];
    assert_eq!(call["level"], json!("verified"));
    assert_eq!(call["decision_effect"], json!("deny"));
    assert_eq!(call["decision_conflict"]["effect"], json!("deny"));
    assert_eq!(call["decision_conflict"]["enforced"], json!(true));
    assert_eq!(report["decision_conflicts"], json!(1));
}

#[test]
fn a_denial_in_another_letter_case_is_still_a_denial() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "DENY", false);

    let report = run(&bundle, None);

    assert_eq!(report["calls"][0]["decision_effect"], json!("DENY"));
    assert_eq!(
        report["calls"][0]["decision_conflict"]["effect"],
        json!("DENY")
    );
    assert_eq!(
        report["calls"][0]["decision_conflict"]["enforced"],
        json!(false)
    );
    assert_eq!(report["decision_conflicts"], json!(1));
}

#[test]
fn an_allowed_call_carries_its_effect_and_no_conflict() {
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "allow", true);

    let report = run(&bundle, None);

    assert_eq!(report["calls"][0]["decision_effect"], json!("allow"));
    assert!(report["calls"][0]["decision_conflict"].is_null());
    assert_eq!(report["decision_conflicts"], json!(0));
}

#[test]
fn the_reported_rejection_follows_file_name_order_not_listing_order() {
    // Two rejected records; the report names the first rejection. Which one is first must be a
    // property of the import (its file names), not of the order the filesystem lists them in.
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("b.tar.gz");
    bundle_with_same_shape_calls(&bundle, 1, "allow", true);
    let out = dir.path().join("audit");
    fs::create_dir_all(&out).unwrap();
    // Written in reverse name order, so creation order and name order disagree.
    fs::write(
        out.join("b.json"),
        serde_json::to_string(&fixture("audit_record_mismatch.json")).unwrap(),
    )
    .unwrap();
    let mut inconsistent = fixture("audit_record_github_deploy_key.json");
    inconsistent["binding_digest"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    fs::write(
        out.join("a.json"),
        serde_json::to_string(&inconsistent).unwrap(),
    )
    .unwrap();

    let report = run(&bundle, Some(&out));

    assert_eq!(report["promoted"], json!(0));
    assert_eq!(
        report["calls"][0]["binding"]["outcome"],
        json!("record_inconsistent"),
        "a.json sorts first, so its rejection is the one reported"
    );
}
