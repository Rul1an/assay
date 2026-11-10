//! A custom pack means what its author wrote. A misspelled rule key used to be dropped: with
//! `event_type` instead of `event_types` the rule lost its scope, ran over every event, and the lint
//! reported no findings. These tests run the real `assay evidence lint` on a verified bundle, in the
//! default form and in the form `assay-action` uses (`--format sarif --fail-on none --pack`), and
//! read the answer at the process boundary: a pack that cannot be read as written is a pack-load
//! failure (exit 3), never a clean lint.
//!
//! Everything is written into a temporary directory, the bundle included, so the test needs no file
//! outside this package.

use assay_evidence::{BundleWriter, EvidenceEvent};
use serde_json::{json, Value};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A verified bundle with one `assay.process.exec` and one `assay.fs.access` event, and none of
/// type `assay.never.emitted`.
fn write_bundle(dir: &Path) -> PathBuf {
    let path = dir.join("bundle.tar.gz");
    let mut writer = BundleWriter::new(File::create(&path).expect("create bundle"));
    for (seq, type_) in ["assay.process.exec", "assay.fs.access"].iter().enumerate() {
        writer.add_event(EvidenceEvent::new(
            *type_,
            "urn:assay:test:lint-pack-fields",
            "run",
            seq as u64,
            json!({}),
        ));
    }
    writer.finish().expect("finish bundle");
    path
}

/// A pack with one rule that requires at least one event in its scope; `scope` is the rule's
/// scope line, written exactly as the author would.
fn write_pack(dir: &Path, file: &str, kind: &str, scope: &str, check: &[&str]) -> PathBuf {
    let mut s = format!(
        "name: lint-pack-fields\nversion: \"1.0.0\"\nkind: {kind}\ndescription: field cells\n\
         author: test\nlicense: Apache-2.0\ndisclaimer: test only\nrequires:\n  assay_min_version: \">=3.3.0\"\n\
         rules:\n  - id: LPF-001\n    severity: error\n    description: needs one event in scope\n"
    );
    if !scope.is_empty() {
        s.push_str(&format!("    {scope}\n"));
    }
    s.push_str("    check:\n");
    for line in check {
        s.push_str(&format!("      {line}\n"));
    }
    let path = dir.join(file);
    std::fs::write(&path, s).expect("write pack");
    path
}

fn lint(bundle: &Path, pack: &Path, format: &str, fail_on: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["evidence", "lint"])
        .arg(bundle)
        .arg("--pack")
        .arg(pack)
        .args(["--format", format, "--fail-on", fail_on])
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

/// The rule IDs of the SARIF results, read from `runs[].results[]` (rule descriptors also carry
/// IDs, so the document containing an ID proves nothing).
fn sarif_result_rule_ids(out: &Output) -> Vec<String> {
    let doc: Value = serde_json::from_slice(&out.stdout).expect("SARIF document on stdout");
    doc["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|run| run["results"].as_array().into_iter().flatten())
        .filter_map(|result| result["ruleId"].as_str().map(str::to_string))
        .collect()
}

const MIN_ONE: &[&str] = &["type: event_count", "min: 1"];

/// The one-typo case and its two controls, in the default text form: the correct key with a scope
/// that matches nothing gives a finding, with a scope that matches one event it passes, and the
/// misspelled key refuses the pack instead of linting every event.
#[test]
fn a_misspelled_rule_scope_refuses_the_pack_instead_of_linting_every_event() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path());
    let restricted = write_pack(
        dir.path(),
        "restricted.yaml",
        "quality",
        "event_types: [assay.never.emitted]",
        MIN_ONE,
    );
    let matching = write_pack(
        dir.path(),
        "matching.yaml",
        "quality",
        "event_types: [assay.process.exec]",
        MIN_ONE,
    );
    let misspelled = write_pack(
        dir.path(),
        "misspelled.yaml",
        "quality",
        "event_type: [assay.never.emitted]",
        MIN_ONE,
    );

    let out = lint(&bundle, &restricted, "text", "error");
    assert_eq!(
        out.status.code(),
        Some(1),
        "restricted scope must yield a finding\n{}",
        text(&out)
    );
    let out = lint(&bundle, &matching, "text", "error");
    assert_eq!(
        out.status.code(),
        Some(0),
        "matching scope must pass\n{}",
        text(&out)
    );

    let out = lint(&bundle, &misspelled, "text", "error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "a misspelled rule key must fail pack loading\n{}",
        text(&out)
    );
    assert!(
        stderr.contains("Pack loading failed") && stderr.contains("`event_type`"),
        "{}",
        text(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("No findings"),
        "{}",
        text(&out)
    );
}

/// The form `assay-action` runs. `--fail-on none` decides which findings gate the run; it must not
/// turn a pack that cannot be read into a clean SARIF document, and the message must not read as a
/// missing pack (the action maps "not found" to `missing_pack`).
#[test]
fn the_action_form_refuses_the_misspelled_pack_regardless_of_fail_on() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path());
    let restricted = write_pack(
        dir.path(),
        "restricted.yaml",
        "quality",
        "event_types: [assay.never.emitted]",
        MIN_ONE,
    );
    let misspelled = write_pack(
        dir.path(),
        "misspelled.yaml",
        "quality",
        "event_type: [assay.never.emitted]",
        MIN_ONE,
    );

    let out = lint(&bundle, &restricted, "sarif", "none");
    assert_eq!(
        out.status.code(),
        Some(0),
        "--fail-on none must not gate the finding\n{}",
        text(&out)
    );
    assert!(
        sarif_result_rule_ids(&out)
            .iter()
            .any(|id| id.ends_with("LPF-001")),
        "the restricted rule must be a SARIF result\n{}",
        text(&out)
    );

    let out = lint(&bundle, &misspelled, "sarif", "none");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "--fail-on none must not decide a pack-load failure\n{}",
        text(&out)
    );
    assert!(
        stderr.contains("`event_type`") && !stderr.contains("not found"),
        "{}",
        text(&out)
    );
}

/// Unknown check types keep their existing contract: a quality pack skips the rule (not a load
/// failure), a compliance pack fails it. Only fields are strict.
#[test]
fn an_unknown_check_type_is_not_a_pack_load_failure() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path());
    let future = &["type: event_kount", "min: 100"];
    let quality = write_pack(dir.path(), "quality.yaml", "quality", "", future);
    let compliance = write_pack(dir.path(), "compliance.yaml", "compliance", "", future);

    let out = lint(&bundle, &quality, "text", "error");
    assert_eq!(
        out.status.code(),
        Some(0),
        "quality pack with an unknown check type\n{}",
        text(&out)
    );
    let out = lint(&bundle, &compliance, "text", "error");
    assert_eq!(
        out.status.code(),
        Some(1),
        "compliance pack with an unknown check type must fail the rule\n{}",
        text(&out)
    );
}
