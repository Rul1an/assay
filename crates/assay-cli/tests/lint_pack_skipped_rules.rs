//! A pack rule this engine cannot run used to disappear without a word. A security or quality pack
//! skipped it (a check type the engine does not know, a conditional shape it does not run, or a
//! rule that asks for a newer engine), logged a warning that no default output showed, and the lint
//! read as if the rule had run and passed. A compliance pack fails such a rule instead, and still
//! does.
//!
//! These tests read the answer where users and consumers read it: the text on stderr, the JSON
//! report and the SARIF document, in the default form and in the form `assay-action` uses
//! (`--format sarif --fail-on none --pack`). A skipped rule is disclosed and is never a finding, so
//! exit codes, findings and `--fail-on` do not move. One test takes the library route, the public
//! `to_sarif` without pack metadata, because that is a second way to the same document.
//!
//! Everything is written into a temporary directory, the bundle included, so the test needs no file
//! outside this package.

use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::packs::load_packs;
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::{BundleWriter, EvidenceEvent, VerifyLimits};
use serde_json::{json, Value};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PACK: &str = "lint-skipped-rules";
const DUP_PACK: &str = "lint-skipped-dup";
const SKIP_NOTE: &str = "ASSAY-LINT-RULE-SKIPPED";

const UNKNOWN: &str = "  - id: LSK-UNKNOWN\n    severity: error\n    description: a check type this engine does not know\n    check:\n      type: event_kount\n      min: 1\n";
const ENGINE: &str = "  - id: LSK-ENGINE\n    severity: error\n    description: asks for a newer engine\n    engine_min_version: \"9.0\"\n    check:\n      type: event_count\n      min: 1\n";
const COND: &str = "  - id: LSK-COND\n    severity: error\n    description: a conditional shape this engine does not run\n    check:\n      type: conditional\n      condition:\n        all:\n          - path: /data/decision\n            equals: allow\n      then:\n        type: event_count\n        min: 1\n";
const FAIL: &str = "  - id: LSK-FAIL\n    severity: warning\n    description: needs an event that never occurs\n    event_types: [assay.never.emitted]\n    check:\n      type: event_count\n      min: 1\n";
const PASS: &str = "  - id: LSK-PASS\n    severity: warning\n    description: needs an event that occurs\n    event_types: [assay.process.exec]\n    check:\n      type: event_count\n      min: 1\n";
const DUP_RUNS: &str = "  - id: LSK-DUP\n    severity: warning\n    description: a supported check that finds nothing in scope\n    event_types: [assay.never.emitted]\n    check:\n      type: event_count\n      min: 1\n";
const DUP_UNKNOWN: &str = "  - id: LSK-DUP\n    severity: warning\n    description: a check type this engine does not know\n    check:\n      type: event_kount\n      min: 1\n";

fn id(rule: &str) -> String {
    format!("{PACK}@1.0.0:{rule}")
}

fn dup_id() -> String {
    format!("{DUP_PACK}@1.0.0:LSK-DUP")
}

/// The three rules of the main pack that this engine cannot run, in pack order, each with a
/// fragment of the reason the engine gives.
fn expected_skips() -> Vec<(String, &'static str)> {
    vec![
        (id("LSK-UNKNOWN"), "Unknown check type"),
        (id("LSK-ENGINE"), "Rule requires engine v9.0"),
        (id("LSK-COND"), "Unsupported conditional shape"),
    ]
}

/// A verified bundle with one `assay.process.exec` and one `assay.fs.access` event, and none of
/// type `assay.never.emitted`.
fn write_bundle(dir: &Path) -> PathBuf {
    let path = dir.join("bundle.tar.gz");
    let mut writer = BundleWriter::new(File::create(&path).expect("create bundle"));
    for (seq, type_) in ["assay.process.exec", "assay.fs.access"].iter().enumerate() {
        writer.add_event(EvidenceEvent::new(
            *type_,
            "urn:assay:test:lint-skipped-rules",
            "run",
            seq as u64,
            json!({}),
        ));
    }
    writer.finish().expect("finish bundle");
    path
}

fn write_pack(dir: &Path, file: &str, name: &str, kind: &str, rules: &[&str]) -> PathBuf {
    let mut s = format!(
        "name: {name}\nversion: \"1.0.0\"\nkind: {kind}\ndescription: skipped rule cells\n\
         author: test\nlicense: Apache-2.0\ndisclaimer: test only\nrequires:\n  assay_min_version: \">=3.3.0\"\n\
         rules:\n"
    );
    for rule in rules {
        s.push_str(rule);
    }
    let path = dir.join(file);
    std::fs::write(&path, s).expect("write pack");
    path
}

/// Runs `assay evidence lint`. `RUST_LOG` is removed so that no logging configuration on the
/// machine can make a skip visible: what the tests read is what every user gets.
fn lint(bundle: &Path, packs: &[&Path], format: &str, fail_on: &str, extra: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.args(["evidence", "lint"]).arg(bundle);
    if !packs.is_empty() {
        let joined = packs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(",");
        cmd.arg("--pack").arg(joined);
    }
    cmd.args(["--format", format, "--fail-on", fail_on])
        .args(extra)
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run assay")
}

fn text(out: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn doc(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("JSON document on stdout ({e})\n{}", text(out)))
}

/// `skipped_rules` as (rule ID, reason) pairs, or `None` when the report has no such key.
fn skipped(report: &Value) -> Option<Vec<(String, String)>> {
    let entries = report.get("skipped_rules")?.as_array()?;
    Some(
        entries
            .iter()
            .map(|e| {
                (
                    e["rule_id"].as_str().unwrap_or_default().to_string(),
                    e["reason"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect(),
    )
}

fn assert_expected_skips(report: &Value, context: &str) {
    let got = skipped(report)
        .unwrap_or_else(|| panic!("the report must carry `skipped_rules`\n{context}"));
    let expected = expected_skips();
    assert_eq!(
        got.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
        "skipped rule IDs, in pack order\n{context}"
    );
    for ((_, reason), (rule, fragment)) in got.iter().zip(&expected) {
        assert!(
            reason.contains(fragment),
            "reason for {rule} must contain {fragment:?}, got {reason:?}\n{context}"
        );
    }
}

/// `(rule_id, severity, message)` of each JSON finding: what a skip must leave untouched.
fn findings(report: &Value) -> Vec<(String, String, String)> {
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            (
                f["rule_id"].as_str().unwrap_or_default().to_string(),
                f["severity"].to_string(),
                f["message"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn finding_ids(report: &Value) -> Vec<String> {
    findings(report).into_iter().map(|(id, _, _)| id).collect()
}

fn sarif_result_rule_ids(sarif: &Value) -> Vec<String> {
    sarif["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|run| run["results"].as_array().into_iter().flatten())
        .filter_map(|result| result["ruleId"].as_str().map(str::to_string))
        .collect()
}

fn notifications(sarif: &Value) -> Vec<Value> {
    sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn skip_notes(sarif: &Value) -> Vec<Value> {
    notifications(sarif)
        .into_iter()
        .filter(|n| n["descriptor"]["id"] == SKIP_NOTE)
        .collect()
}

/// The rule each skip notification refers to, resolved the way a SARIF consumer resolves it:
/// `associatedRule.index` into `tool.driver.rules`, which must hold a descriptor with the same ID.
/// `None` marks a notification whose reference does not resolve.
fn resolved_skip_rules(sarif: &Value) -> Vec<Option<String>> {
    let rules = sarif["runs"][0]["tool"]["driver"]["rules"].as_array();
    skip_notes(sarif)
        .iter()
        .map(|note| {
            let rule_id = note["associatedRule"]["id"].as_str()?;
            let index = usize::try_from(note["associatedRule"]["index"].as_u64()?).ok()?;
            let descriptor = rules?.get(index)?;
            if descriptor["id"].as_str()? == rule_id {
                Some(rule_id.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Every skipped rule is a resolvable warning notification that names the rule and its reason,
/// with exactly one descriptor per skipped rule.
fn assert_expected_skip_notes(sarif: &Value, context: &str) {
    let expected = expected_skips();
    let notes = skip_notes(sarif);
    assert_eq!(
        notes.len(),
        expected.len(),
        "one `{SKIP_NOTE}` notification per skipped rule\n{context}"
    );
    assert_eq!(
        resolved_skip_rules(sarif),
        expected
            .iter()
            .map(|(id, _)| Some(id.clone()))
            .collect::<Vec<_>>(),
        "each notification must resolve through associatedRule.index to its own descriptor\n{context}"
    );
    let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for ((rule, fragment), note) in expected.iter().zip(&notes) {
        assert_eq!(note["level"], "warning", "{rule}\n{context}");
        let message = note["message"]["text"].as_str().unwrap_or_default();
        assert!(
            message.contains(rule.as_str()) && message.contains(fragment),
            "notification for {rule} must name it and its reason, got {message:?}\n{context}"
        );
        assert_eq!(
            rules.iter().filter(|r| r["id"] == rule.as_str()).count(),
            1,
            "exactly one descriptor for {rule}\n{context}"
        );
    }
}

/// Where the text output states what was found: "No findings." or the summary after findings.
fn verdict_position(stderr: &str) -> Option<usize> {
    stderr
        .find("No findings.")
        .or_else(|| stderr.find("Summary:"))
}

fn assert_text_block(stderr: &str, context: &str) {
    let block = stderr
        .find("Rules not run: 3")
        .unwrap_or_else(|| panic!("text output must say how many rules did not run\n{context}"));
    for (rule, fragment) in expected_skips() {
        assert!(
            stderr
                .lines()
                .any(|line| line.contains(&rule) && line.contains(fragment)),
            "text output must name {rule} with its reason\n{context}"
        );
    }
    let verdict = verdict_position(stderr).expect("a verdict line");
    assert!(
        block < verdict,
        "the rules that did not run come before the verdict\n{context}"
    );
}

struct Fixture {
    _dir: tempfile::TempDir,
    bundle: PathBuf,
    /// Quality pack: the three skipped rules, a failing rule and a passing rule.
    q: PathBuf,
    /// Quality pack: the three skipped rules and a passing rule.
    q0: PathBuf,
    /// Quality pack: a failing rule and a passing rule.
    v: PathBuf,
    /// Quality pack: a passing rule only.
    p: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path());
    let q = write_pack(
        dir.path(),
        "q.yaml",
        PACK,
        "quality",
        &[UNKNOWN, ENGINE, COND, FAIL, PASS],
    );
    let q0 = write_pack(
        dir.path(),
        "q0.yaml",
        PACK,
        "quality",
        &[UNKNOWN, ENGINE, COND, PASS],
    );
    let v = write_pack(dir.path(), "v.yaml", PACK, "quality", &[FAIL, PASS]);
    let p = write_pack(dir.path(), "p.yaml", PACK, "quality", &[PASS]);
    Fixture {
        _dir: dir,
        bundle,
        q,
        q0,
        v,
        p,
    }
}

/// The default text form names every rule that did not run, with its reason, before the verdict.
#[test]
fn text_output_names_each_rule_that_did_not_run() {
    let f = fixture();

    let out = lint(&f.bundle, &[&f.q], "text", "error", &[]);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        err.contains(&format!("Packs: {PACK}@1.0.0")) && err.contains(&id("LSK-FAIL")),
        "the pack must have run (its failing rule is a finding)\n{}",
        text(&out)
    );
    assert_text_block(&err, &text(&out));

    let out = lint(&f.bundle, &[&f.q0], "text", "error", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert_text_block(&stderr(&out), &text(&out));
}

/// The JSON report lists the rules that did not run, and the findings and summary stay exactly what
/// the same pack without those rules produces.
#[test]
fn json_report_lists_each_rule_that_did_not_run() {
    let f = fixture();

    let out = lint(&f.bundle, &[&f.q], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let q = doc(&out);
    let ids = finding_ids(&q);
    assert!(ids.contains(&id("LSK-FAIL")), "{}", text(&out));
    for (rule, _) in expected_skips() {
        assert!(
            !ids.contains(&rule),
            "a skipped rule is never a finding\n{}",
            text(&out)
        );
    }

    let out_v = lint(&f.bundle, &[&f.v], "json", "none", &[]);
    let v = doc(&out_v);
    assert_eq!(
        findings(&q),
        findings(&v),
        "findings must not move\n{}",
        text(&out)
    );
    assert_eq!(
        q["summary"],
        v["summary"],
        "summary must not move\n{}",
        text(&out)
    );

    assert_expected_skips(&q, &text(&out));
}

/// The form `assay-action` runs: each rule that did not run is a warning-level tool-execution
/// notification whose rule reference resolves, while results and `executionSuccessful` stay as
/// they were.
#[test]
fn sarif_action_form_discloses_each_rule_that_did_not_run() {
    let f = fixture();

    let out = lint(&f.bundle, &[&f.q], "sarif", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let sarif = doc(&out);
    let results = sarif_result_rule_ids(&sarif);
    assert!(results.contains(&id("LSK-FAIL")), "{}", text(&out));
    for (rule, _) in expected_skips() {
        assert!(!results.contains(&rule), "{}", text(&out));
    }
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"],
        json!(true),
        "{}",
        text(&out)
    );

    assert_expected_skip_notes(&sarif, &text(&out));
}

/// The security kind skips the same way the quality kind does.
#[test]
fn a_security_pack_discloses_its_skipped_conditional() {
    let f = fixture();
    let s = write_pack(
        f.bundle.parent().unwrap(),
        "s.yaml",
        PACK,
        "security",
        &[COND, FAIL],
    );

    let out = lint(&f.bundle, &[&s], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let report = doc(&out);
    assert!(
        finding_ids(&report).contains(&id("LSK-FAIL")),
        "{}",
        text(&out)
    );
    let got = skipped(&report)
        .unwrap_or_else(|| panic!("the report must carry `skipped_rules`\n{}", text(&out)));
    assert_eq!(got.len(), 1, "{}", text(&out));
    assert_eq!(got[0].0, id("LSK-COND"), "{}", text(&out));
    assert!(
        got[0].1.contains("Unsupported conditional shape"),
        "{}",
        text(&out)
    );
}

/// A pack whose rules all ran: no notification, no text block, and an empty list rather than a
/// missing key, so a consumer can tell "nothing skipped" from "not reported".
#[test]
fn a_pack_whose_rules_all_ran_reports_an_empty_list() {
    let f = fixture();

    let out = lint(&f.bundle, &[&f.v], "sarif", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let sarif = doc(&out);
    assert!(
        sarif_result_rule_ids(&sarif).contains(&id("LSK-FAIL")),
        "{}",
        text(&out)
    );
    assert!(skip_notes(&sarif).is_empty(), "{}", text(&out));

    let out = lint(&f.bundle, &[&f.v], "text", "error", &[]);
    assert!(!stderr(&out).contains("Rules not run"), "{}", text(&out));

    let out = lint(&f.bundle, &[&f.v], "json", "none", &[]);
    assert_eq!(
        doc(&out).get("skipped_rules"),
        Some(&json!([])),
        "{}",
        text(&out)
    );
}

/// A compliance pack keeps failing a rule it cannot run, and that rule is a finding, not a skip.
#[test]
fn a_compliance_pack_still_fails_the_rule_and_does_not_list_it_as_skipped() {
    let f = fixture();
    let c = write_pack(
        f.bundle.parent().unwrap(),
        "c.yaml",
        PACK,
        "compliance",
        &[UNKNOWN],
    );

    let out = lint(&f.bundle, &[&c], "text", "error", &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "compliance must fail the rule\n{}",
        text(&out)
    );

    let out = lint(&f.bundle, &[&c], "sarif", "none", &[]);
    let sarif = doc(&out);
    assert!(
        sarif_result_rule_ids(&sarif).contains(&id("LSK-UNKNOWN")),
        "{}",
        text(&out)
    );
    assert!(skip_notes(&sarif).is_empty(), "{}", text(&out));

    let out = lint(&f.bundle, &[&c], "json", "none", &[]);
    let report = doc(&out);
    assert!(
        findings(&report)
            .iter()
            .any(|(rule, _, message)| *rule == id("LSK-UNKNOWN")
                && message.contains("Cannot execute rule")),
        "{}",
        text(&out)
    );
    assert_eq!(
        report.get("skipped_rules"),
        Some(&json!([])),
        "{}",
        text(&out)
    );
}

/// A skip is not a finding: under every `--fail-on`, a pack with skipped rules exits exactly as
/// the same pack without them, in text and in JSON.
#[test]
fn a_skipped_rule_moves_no_exit_code() {
    let f = fixture();
    for fail_on in ["info", "warn", "error", "none"] {
        for format in ["text", "json"] {
            let with_skips = lint(&f.bundle, &[&f.q0], format, fail_on, &[]);
            let without = lint(&f.bundle, &[&f.p], format, fail_on, &[]);
            assert_eq!(
                with_skips.status.code(),
                without.status.code(),
                "--fail-on {fail_on} --format {format}\n{}\n---\n{}",
                text(&with_skips),
                text(&without)
            );
            if fail_on == "none" {
                assert_eq!(with_skips.status.code(), Some(0), "{}", text(&with_skips));
            }
        }
    }
    let with_skips = doc(&lint(&f.bundle, &[&f.q0], "json", "none", &[]));
    let without = doc(&lint(&f.bundle, &[&f.p], "json", "none", &[]));
    assert_eq!(findings(&with_skips), findings(&without));
    assert_eq!(with_skips["summary"], without["summary"]);
}

/// A cap of zero drops every finding but none of the rules that did not run; the truncation
/// notification keeps its place, and the skipped rules keep their descriptors although no finding
/// carries them.
#[test]
fn a_zero_cap_drops_findings_but_not_the_rules_that_did_not_run() {
    let f = fixture();
    let cap = ["--max-results", "0"];

    let out = lint(&f.bundle, &[&f.q], "json", "none", &cap);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let report = doc(&out);
    assert!(findings(&report).is_empty(), "{}", text(&out));
    assert_eq!(report["truncated"], json!(true), "{}", text(&out));
    assert_expected_skips(&report, &text(&out));

    let out = lint(&f.bundle, &[&f.q], "sarif", "none", &cap);
    let sarif = doc(&out);
    assert!(sarif_result_rule_ids(&sarif).is_empty(), "{}", text(&out));
    let notes = notifications(&sarif);
    assert_eq!(
        notes.first().map(|n| n["descriptor"]["id"].clone()),
        Some(json!("ASSAY-LINT-TRUNCATED")),
        "the truncation notification stays first\n{}",
        text(&out)
    );
    assert_expected_skip_notes(&sarif, &text(&out));
    assert_eq!(notes.len(), 1 + expected_skips().len(), "{}", text(&out));

    let out = lint(&f.bundle, &[&f.q], "text", "error", &cap);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let err = stderr(&out);
    assert!(err.contains("Results truncated"), "{}", text(&out));
    assert_text_block(&err, &text(&out));
}

/// Two packs with one name and version define the same canonical rule ID. The first in `--pack`
/// order is the one that runs, and only the definition that ran can be reported as skipped.
#[test]
fn only_the_definition_that_runs_can_be_reported_as_skipped() {
    let f = fixture();
    let dir = f.bundle.parent().unwrap();
    let runs = write_pack(dir, "dup-runs.yaml", DUP_PACK, "quality", &[DUP_RUNS]);
    let unknown = write_pack(dir, "dup-unknown.yaml", DUP_PACK, "quality", &[DUP_UNKNOWN]);

    let out = lint(&f.bundle, &[&runs, &unknown], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let report = doc(&out);
    assert_eq!(
        finding_ids(&report)
            .iter()
            .filter(|rule| **rule == dup_id())
            .count(),
        1,
        "the first definition runs and finds nothing in scope\n{}",
        text(&out)
    );
    assert_eq!(
        report.get("skipped_rules"),
        Some(&json!([])),
        "{}",
        text(&out)
    );

    let out = lint(&f.bundle, &[&unknown, &runs], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let report = doc(&out);
    assert!(
        !finding_ids(&report).contains(&dup_id()),
        "the first definition is the one that does not run\n{}",
        text(&out)
    );
    let got = skipped(&report)
        .unwrap_or_else(|| panic!("the report must carry `skipped_rules`\n{}", text(&out)));
    assert_eq!(got.len(), 1, "{}", text(&out));
    assert_eq!(got[0].0, dup_id(), "{}", text(&out));
    assert!(got[0].1.contains("Unknown check type"), "{}", text(&out));
}

/// Built-in packs on this bundle skip nothing: the default run and the one built-in pack with a
/// conditional rule report an empty list.
#[test]
fn built_in_packs_on_this_bundle_skip_nothing() {
    let f = fixture();

    let out = lint(&f.bundle, &[], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert_eq!(
        doc(&out).get("skipped_rules"),
        Some(&json!([])),
        "{}",
        text(&out)
    );

    let mandate = Path::new("mandate-baseline");
    let out = lint(&f.bundle, &[mandate], "json", "none", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert_eq!(
        doc(&out).get("skipped_rules"),
        Some(&json!([])),
        "{}",
        text(&out)
    );
}

/// The library route: `to_sarif` with default options has no pack metadata, and its skip
/// notifications must still resolve to descriptors in the same document.
#[test]
fn the_library_sarif_route_discloses_skips_without_pack_metadata() {
    let f = fixture();
    let packs = load_packs(&[f.q.display().to_string()]).expect("load the pack");
    let result = lint_bundle_with_options(
        File::open(&f.bundle).expect("open bundle"),
        VerifyLimits::default(),
        LintOptions {
            packs,
            max_results: None,
            bundle_path: None,
        },
    )
    .expect("lint should succeed");
    assert!(result.pack_meta.is_some(), "the pack must have run");

    let sarif = to_sarif(&result.report);
    let context = serde_json::to_string_pretty(&sarif).unwrap_or_default();
    assert!(
        sarif_result_rule_ids(&sarif).contains(&id("LSK-FAIL")),
        "{context}"
    );
    assert_expected_skip_notes(&sarif, &context);
}
