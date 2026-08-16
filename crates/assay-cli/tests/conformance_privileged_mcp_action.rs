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
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_ID: &str = "privileged-mcp-action/v0";
const REPORT_NON_CLAIMS: [&str; 4] = [
    "allow does not prove upstream delivery",
    "deny does not establish maliciousness",
    "caller-visible denial does not prove external side-effect absence",
    "bundle integrity does not upgrade source class",
];

const INTEGRITY_NEXT_STEP: &str = "Obtain an undamaged bundle from its producer; the content this bundle carries does not match what it records";
const CONTRACT_NEXT_STEP: &str = "Obtain or reissue evidence that conforms to the declared bundle contract; this bundle was readable and does not satisfy that contract";
const PROFILE_INVALID_NEXT_STEP: &str = "Obtain or reissue evidence whose records satisfy the named evidence profile; per-violation details are in findings";
const LIMIT_NEXT_STEP: &str = "Verification stopped at a configured ceiling and reached no verdict; obtain a smaller bundle from its producer, or raise the ceiling deliberately and repeat the inspection";
const PATH_NEXT_STEP: &str = "An archive member path was refused as unsafe to extract; obtain a bundle whose member paths stay inside the extraction root from its producer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedDiagnosis {
    Absent,
    Present {
        reason_code: &'static str,
        next_step: &'static str,
    },
}

/// Pin every committed vector by exact id so a new corpus member cannot inherit a class.
fn expected_diagnosis(id: &str) -> ExpectedDiagnosis {
    match id {
        "ok-001-deny-bound-observation"
        | "ok-002-deny-observation-missing"
        | "ok-003-allow-no-outcome-observation"
        | "ok-004-allow-with-diagnostic-establish"
        | "ok-005-allow-contradicted-by-denial" => ExpectedDiagnosis::Absent,
        "bad-101-tampered-bundle" => ExpectedDiagnosis::Present {
            reason_code: "E_EVIDENCE_INTEGRITY",
            next_step: INTEGRITY_NEXT_STEP,
        },
        "bad-109-bundle-id-mismatch" => ExpectedDiagnosis::Present {
            reason_code: "E_EVIDENCE_CONTRACT",
            next_step: CONTRACT_NEXT_STEP,
        },
        "bad-102-missing-target-digest"
        | "bad-103-two-decisions"
        | "bad-104-unknown-schema"
        | "bad-105-observation-binding-mismatch"
        | "bad-106-fail-closed-inconsistent"
        | "bad-107-unknown-decision-value"
        | "bad-108-observation-without-decision" => ExpectedDiagnosis::Present {
            reason_code: "E_EVIDENCE_PROFILE_INVALID",
            next_step: PROFILE_INVALID_NEXT_STEP,
        },
        other => panic!("unmapped privileged-mcp-action vector {other}"),
    }
}

fn assert_diagnosis(id: &str, report: &Value, diagnosis: ExpectedDiagnosis) {
    match diagnosis {
        ExpectedDiagnosis::Absent => {
            assert!(
                report.get("reason_code").is_none(),
                "{id}: success diagnosis must be absent, not null or empty; got {:?}",
                report.get("reason_code")
            );
            assert!(
                report.get("next_step").is_none(),
                "{id}: success next_step must be absent, not null or empty; got {:?}",
                report.get("next_step")
            );
        }
        ExpectedDiagnosis::Present {
            reason_code,
            next_step,
        } => {
            assert_eq!(
                report["reason_code"].as_str(),
                Some(reason_code),
                "{id}: reason_code"
            );
            let published = report["next_step"]
                .as_str()
                .unwrap_or_else(|| panic!("{id}: next_step must be a non-empty string"));
            assert_eq!(published, next_step, "{id}: next_step");
            assert!(!published.is_empty(), "{id}: next_step must not be empty");
            assert!(
                !published.starts_with("Run:") && !published.starts_with("Run argv:"),
                "{id}: next_step must stay context-invariant prose: {published}"
            );
            assert!(
                !published.contains("verify-privileged-mcp-action")
                    && !published.contains('/')
                    && !published.contains('\\'),
                "{id}: next_step must not interpolate a command or path: {published}"
            );
        }
    }
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/privileged-mcp-action-v0")
}

fn verify_raw(bundle: &Path) -> (Vec<u8>, i32) {
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "json"])
        .output()
        .expect("run verifier");
    (output.stdout, output.status.code().expect("exit code"))
}

fn verify(bundle: &Path) -> (Value, i32) {
    let (stdout, exit_code) = verify_raw(bundle);
    let report: Value = serde_json::from_slice(&stdout)
        .unwrap_or_else(|e| panic!("report for {} is not JSON: {e}", bundle.display()));
    (report, exit_code)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// Pre-change success stdout pins measured 2026-08-16 from the assay binary built at
/// `656e692ff02a00052d833d0676e229452c3f23fe` in temporary sibling worktree
/// `/Users/roelschuurkes/wt-2165-prechange-bytes` (`cargo build -p assay-cli`).
/// Command: `assay evidence verify-privileged-mcp-action --format json <bundle>`.
/// Corpus: committed `conformance/privileged-mcp-action-v0/vectors/` (byte-identical
/// to that commit). Digest is SHA-256 of raw stdout with no trailing-newline strip.
/// Three unique forms: ok-001; shared ok-002/003/004; ok-005.
fn prechange_success_stdout_pin(id: &str) -> (usize, &'static str) {
    match id {
        "ok-001-deny-bound-observation" => (
            782,
            "9832de15214edcb07ff9006897b95bdbd202a256918b5dd8086617e6afb85329",
        ),
        "ok-002-deny-observation-missing"
        | "ok-003-allow-no-outcome-observation"
        | "ok-004-allow-with-diagnostic-establish" => (
            740,
            "6a861a7c406eafb4ce450e1755c5c645e3ad4d97e5ca2fec618c123432cf4132",
        ),
        "ok-005-allow-contradicted-by-denial" => (
            1000,
            "f949e7f2332fdc993eb361020cf2fcfaaba85cb5c14aa28f9ac34a811f4d5392",
        ),
        other => panic!("no pre-change stdout pin for {other}"),
    }
}

#[test]
fn conformance_corpus_reproduces_all_expected_outcomes() {
    let corpus = corpus_dir();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(corpus.join("MANIFEST.json")).expect("read MANIFEST.json"),
    )
    .expect("parse MANIFEST.json");

    let vectors = manifest["vectors"].as_array().expect("vectors array");
    assert_eq!(vectors.len(), 14, "the v0 corpus carries 14 vectors");

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
        assert_diagnosis(id, &report, expected_diagnosis(id));

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
    assert_diagnosis(
        "ok-005-allow-contradicted-by-denial",
        &report,
        ExpectedDiagnosis::Absent,
    );
}

#[test]
fn success_json_stdout_is_byte_identical_to_prechange() {
    let corpus = corpus_dir();
    let cases = [
        "ok-001-deny-bound-observation",
        "ok-002-deny-observation-missing",
        "ok-003-allow-no-outcome-observation",
        "ok-004-allow-with-diagnostic-establish",
        "ok-005-allow-contradicted-by-denial",
    ];
    for id in cases {
        let bundle = corpus.join(format!("vectors/{id}.bundle.tar.gz"));
        let (stdout, exit_code) = verify_raw(&bundle);
        assert_eq!(exit_code, 0, "{id}: success exits 0");
        let (len, digest) = prechange_success_stdout_pin(id);
        assert_eq!(stdout.len(), len, "{id}: raw stdout length vs pre-change");
        assert_eq!(
            sha256_hex(&stdout),
            digest,
            "{id}: raw stdout sha256 vs pre-change"
        );
    }
}

fn verify_table(bundle: &Path) -> (String, i32) {
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "table"])
        .output()
        .expect("run verifier table");
    (
        String::from_utf8(output.stdout).expect("table stdout utf-8"),
        output.status.code().expect("exit code"),
    )
}

#[test]
fn table_output_carries_diagnosis_only_when_present() {
    let corpus = corpus_dir();
    let (ok_table, ok_exit) =
        verify_table(&corpus.join("vectors/ok-001-deny-bound-observation.bundle.tar.gz"));
    assert_eq!(ok_exit, 0);
    assert!(
        !ok_table.contains("E_EVIDENCE_") && !ok_table.contains("Next step:"),
        "success table must omit diagnosis, got:\n{ok_table}"
    );

    let cases = [
        (
            "vectors/bad-101-tampered-bundle.bundle.tar.gz",
            "E_EVIDENCE_INTEGRITY",
            INTEGRITY_NEXT_STEP,
        ),
        (
            "vectors/bad-109-bundle-id-mismatch.bundle.tar.gz",
            "E_EVIDENCE_CONTRACT",
            CONTRACT_NEXT_STEP,
        ),
        (
            "vectors/bad-102-missing-target-digest.bundle.tar.gz",
            "E_EVIDENCE_PROFILE_INVALID",
            PROFILE_INVALID_NEXT_STEP,
        ),
    ];
    for (file, reason, next_step) in cases {
        let (table, exit_code) = verify_table(&corpus.join(file));
        assert_eq!(exit_code, 2, "{file}");
        assert!(
            table.contains(reason),
            "{file}: table must publish {reason}, got:\n{table}"
        );
        assert!(
            table.contains(next_step),
            "{file}: table must publish next_step, got:\n{table}"
        );
    }
}

#[test]
fn diagnosis_mutations_keep_the_three_facts_apart() {
    let corpus = corpus_dir();
    let (ok_005, _) =
        verify(&corpus.join("vectors/ok-005-allow-contradicted-by-denial.bundle.tar.gz"));
    assert_ne!(
        ok_005.get("reason_code").and_then(Value::as_str),
        Some("E_EVIDENCE_PROFILE_INVALID"),
        "ok-005 is a valid refuted-cell report, not a profile-invalid failure"
    );
    assert!(ok_005.get("reason_code").is_none());

    let (bad_101, _) = verify(&corpus.join("vectors/bad-101-tampered-bundle.bundle.tar.gz"));
    assert_eq!(bad_101["reason_code"], "E_EVIDENCE_INTEGRITY");
    assert_ne!(bad_101["reason_code"], "E_EVIDENCE_CONTRACT");
    assert!(bad_101.get("claims").is_none());

    let (bad_109, _) = verify(&corpus.join("vectors/bad-109-bundle-id-mismatch.bundle.tar.gz"));
    assert_eq!(bad_109["reason_code"], "E_EVIDENCE_CONTRACT");
    assert_ne!(bad_109["reason_code"], "E_EVIDENCE_INTEGRITY");
    assert!(bad_109.get("claims").is_none());

    let (bad_102, _) = verify(&corpus.join("vectors/bad-102-missing-target-digest.bundle.tar.gz"));
    assert_eq!(bad_102["reason_code"], "E_EVIDENCE_PROFILE_INVALID");
    assert_ne!(bad_102["reason_code"], "E_EVIDENCE_CONTRACT");
    assert_ne!(bad_102["reason_code"], "E_EVIDENCE_INTEGRITY");
    assert!(bad_102.get("claims").is_none());
    assert!(
        !bad_102["next_step"].as_str().unwrap_or("").is_empty(),
        "profile-invalid next_step must not be empty"
    );
}

fn write_named_member_bundle(dir: &Path, member: &str) -> PathBuf {
    let dest = dir.join("synthetic.bundle.tar.gz");
    let script = dir.join("write_tar.py");
    std::fs::write(
        &script,
        "import io, sys, tarfile\n\
         dest, member = sys.argv[1], sys.argv[2]\n\
         buf = io.BytesIO()\n\
         archive = tarfile.open(fileobj=buf, mode='w:gz')\n\
         info = tarfile.TarInfo(member)\n\
         info.size = 1\n\
         archive.addfile(info, io.BytesIO(b'x'))\n\
         archive.close()\n\
         open(dest, 'wb').write(buf.getvalue())\n",
    )
    .expect("write python helper");
    let status = std::process::Command::new("python3")
        .args([
            script.as_os_str(),
            dest.as_os_str(),
            std::ffi::OsStr::new(member),
        ])
        .status()
        .expect("python3 tarfile");
    assert!(status.success(), "python3 failed to write {member}");
    dest
}

/// Synthetic command-level cases sit beside the 14 corpus vectors. They do not fold
/// Limits/Security into Integrity/Contract/Unreadable. Command-level drive covers the
/// reachable traversal code; AbsolutePath is a non-claim here.
#[test]
fn synthetic_limit_and_path_cases_consume_their_own_codes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let long_name = "a".repeat(257);
    let limit_bundle = write_named_member_bundle(tmp.path(), &long_name);
    let (limit_report, limit_exit) = verify(&limit_bundle);
    assert_eq!(limit_exit, 2);
    assert_diagnosis(
        "synthetic-limit-path-length",
        &limit_report,
        ExpectedDiagnosis::Present {
            reason_code: "E_EVIDENCE_LIMIT_EXCEEDED",
            next_step: LIMIT_NEXT_STEP,
        },
    );
    assert_ne!(limit_report["reason_code"], "E_EVIDENCE_INTEGRITY");
    assert_ne!(limit_report["reason_code"], "E_EVIDENCE_CONTRACT");
    assert_ne!(limit_report["reason_code"], "E_EVIDENCE_UNREADABLE");
    assert_ne!(limit_report["reason_code"], "E_EVIDENCE_PATH_REJECTED");
    assert!(limit_report.get("claims").is_none());
    assert_eq!(limit_report["bundle_integrity"], "fail");

    let path_dir = tmp.path().join("path");
    std::fs::create_dir(&path_dir).expect("path dir");
    let path_bundle = write_named_member_bundle(&path_dir, "../x");
    let (path_report, path_exit) = verify(&path_bundle);
    assert_eq!(path_exit, 2);
    assert_diagnosis(
        "synthetic-security-path-traversal",
        &path_report,
        ExpectedDiagnosis::Present {
            reason_code: "E_EVIDENCE_PATH_REJECTED",
            next_step: PATH_NEXT_STEP,
        },
    );
    assert_ne!(path_report["reason_code"], "E_EVIDENCE_LIMIT_EXCEEDED");
    assert_ne!(path_report["reason_code"], "E_EVIDENCE_INTEGRITY");
    assert_ne!(path_report["reason_code"], "E_EVIDENCE_CONTRACT");
    assert_ne!(path_report["reason_code"], "E_EVIDENCE_UNREADABLE");
    assert!(path_report.get("claims").is_none());
    assert_eq!(path_report["bundle_integrity"], "fail");

    let (limit_table, _) = verify_table(&limit_bundle);
    assert!(
        limit_table.contains("E_EVIDENCE_LIMIT_EXCEEDED") && limit_table.contains(LIMIT_NEXT_STEP),
        "limit table must publish diagnosis, got:\n{limit_table}"
    );
    let (path_table, _) = verify_table(&path_bundle);
    assert!(
        path_table.contains("E_EVIDENCE_PATH_REJECTED") && path_table.contains(PATH_NEXT_STEP),
        "path table must publish diagnosis, got:\n{path_table}"
    );
}

#[test]
fn security_absolute_path_is_not_required_for_command_completeness() {
    let errors = include_str!("../../assay-evidence/src/bundle/writer_next/errors.rs");
    assert!(
        errors.contains("SecurityAbsolutePath,"),
        "the enum still declares SecurityAbsolutePath"
    );
    let verify_src = include_str!("../../assay-evidence/src/bundle/writer_next/verify.rs");
    assert!(
        verify_src.contains("SecurityPathTraversal"),
        "verify.rs must still construct the reachable SecurityPathTraversal code"
    );
    assert!(
        !verify_src.contains("SecurityAbsolutePath"),
        "verify.rs today constructs SecurityPathTraversal only; this is not a reachability claim for AbsolutePath"
    );
}
