//! Conformance harness for the privileged-mcp-action/v0 open profile.
//!
//! Runs `assay evidence verify-privileged-mcp-action` against every vector bundle in
//! `conformance/privileged-mcp-action-v0/` and asserts the normative comparison surface of the
//! corpus MANIFEST: `expected.bundle_integrity`, `expected.verdict`, and (for accepts) the full
//! `expected.claims` object. `first_failure_informative` codes are the generator's own vocabulary
//! and are deliberately NOT compared: an independent implementation is scored on outcomes.
//!
//! Report-shape invariants asserted on every vector: the report schema and profile ids, the four
//! fixed non-claims verbatim, verdict absent on integrity failure, claims absent unless the
//! verdict is valid, and the exit-code convention (0 iff pass + valid, else 2; a refuted claim
//! cell still exits 0 because consumers gate on cells, not on the process exit).

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_ID: &str = "privileged-mcp-action/v0";
const REPORT_NON_CLAIMS: [&str; 4] = [
    "allow does not prove upstream delivery",
    "deny does not establish maliciousness",
    "caller-visible denial does not prove external side-effect absence",
    "bundle integrity does not upgrade source class",
];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/privileged-mcp-action-v0")
}

fn verify(bundle: &Path) -> (Value, i32) {
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "json"])
        .output()
        .expect("run verifier");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("report for {} is not JSON: {e}", bundle.display()));
    (report, output.status.code().expect("exit code"))
}

#[test]
fn conformance_corpus_reproduces_all_expected_outcomes() {
    let corpus = corpus_dir();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(corpus.join("MANIFEST.json")).expect("read MANIFEST.json"),
    )
    .expect("parse MANIFEST.json");

    let vectors = manifest["vectors"].as_array().expect("vectors array");
    assert_eq!(vectors.len(), 13, "the v0 corpus carries 13 vectors");

    for vector in vectors {
        let id = vector["id"].as_str().expect("vector id");
        let file = corpus.join(vector["file"].as_str().expect("vector file"));
        let expected = &vector["expected"];
        let (report, exit_code) = verify(&file);

        // Report-shape invariants, every vector.
        assert_eq!(report["schema"], REPORT_SCHEMA, "{id}: report schema");
        assert_eq!(report["profile"], PROFILE_ID, "{id}: report profile");
        assert_eq!(
            report["non_claims"],
            serde_json::json!(REPORT_NON_CLAIMS.to_vec()),
            "{id}: the four fixed non-claims must be present verbatim"
        );

        let expected_integrity = expected["bundle_integrity"].as_str().unwrap();
        assert_eq!(
            report["bundle_integrity"].as_str(),
            Some(expected_integrity),
            "{id}: bundle_integrity"
        );

        if expected_integrity == "fail" {
            // On integrity failure nothing below stage 1 is consumed: the spec requires verdict
            // and claims to be ABSENT (the corpus MANIFEST's `verdict: invalid` for the tamper
            // vector means "not accepted", which an absent verdict satisfies).
            assert!(
                report.get("verdict").is_none(),
                "{id}: verdict absent on fail"
            );
            assert!(
                report.get("claims").is_none(),
                "{id}: claims absent on fail"
            );
            assert_eq!(exit_code, 2, "{id}: integrity failure exits 2");
            continue;
        }

        let expected_verdict = expected["verdict"].as_str().unwrap();
        assert_eq!(
            report["verdict"].as_str(),
            Some(expected_verdict),
            "{id}: verdict"
        );

        if expected_verdict == "valid" {
            // Accept vectors compare the FULL expected claim matrix, byte-for-byte as JSON values
            // (statuses, source classes on confirmed/refuted cells, no source class on incomplete).
            assert_eq!(&report["claims"], &expected["claims"], "{id}: claim matrix");
            assert_eq!(
                exit_code, 0,
                "{id}: valid verdict exits 0 (refuted cells included)"
            );
        } else {
            assert!(
                report.get("claims").is_none(),
                "{id}: claims absent on invalid verdict"
            );
            assert!(
                report["findings"].as_array().is_some_and(|f| !f.is_empty()),
                "{id}: an invalid verdict reports at least one free-form finding"
            );
            assert_eq!(exit_code, 2, "{id}: invalid verdict exits 2");
        }
    }
}

#[test]
fn contradiction_vector_reports_the_contradiction_finding() {
    // ok-005 is the refuted-cell vector: exit 0, but the caller_visible_outcome_contradiction
    // finding must be present so a consumer that only reads findings still sees the conflict.
    let bundle = corpus_dir().join("vectors/ok-005-allow-contradicted-by-denial.bundle.tar.gz");
    let (report, exit_code) = verify(&bundle);
    assert_eq!(exit_code, 0);
    assert_eq!(
        report["claims"]["caller_visible_denial"]["status"],
        "refuted"
    );
    let findings = report["findings"].as_array().expect("findings");
    assert!(
        findings
            .iter()
            .any(|f| f["id"] == "caller_visible_outcome_contradiction"),
        "refuted caller-visible outcome must carry the contradiction finding, got {findings:?}"
    );
}
