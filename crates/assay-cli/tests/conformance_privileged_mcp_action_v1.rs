//! Focused conformance for privileged-mcp-action/v1.
//!
//! Default invocation stays v0. v1 requires `--profile-version v1`.
//! Historical v0 corpus digest remains byte-exact.

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_V0: &str = "privileged-mcp-action/v0";
const PROFILE_V1: &str = "privileged-mcp-action/v1";
const V0_CORPUS_DIGEST: &str =
    "sha256:cb58ce91863f52e0568742b977f0642158453ec11bbcd25821f9171dccd03342";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn v0_corpus() -> PathBuf {
    repo_root().join("conformance/privileged-mcp-action-v0")
}

fn v1_corpus() -> PathBuf {
    repo_root().join("conformance/privileged-mcp-action-v1")
}

fn verify(bundle: &Path, profile_version: Option<&str>) -> (Value, i32) {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.args(["evidence", "verify-privileged-mcp-action"])
        .arg(bundle)
        .args(["--format", "json"]);
    if let Some(version) = profile_version {
        cmd.args(["--profile-version", version]);
    }
    let output = cmd.output().expect("run verifier");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("report for {} is not JSON: {e}", bundle.display()));
    (report, output.status.code().expect("exit code"))
}

#[test]
fn v0_corpus_digest_remains_pinned() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(v0_corpus().join("MANIFEST.json")).expect("v0 MANIFEST"),
    )
    .expect("parse v0 MANIFEST");
    assert_eq!(manifest["corpus_digest"], V0_CORPUS_DIGEST);
    assert_eq!(manifest["vectors"].as_array().map(Vec::len), Some(14));
}

#[test]
fn default_invocation_pins_v0_on_v1_corpus_accept() {
    let bundle = v1_corpus().join("vectors/ok-002-deny-observation-missing.bundle.tar.gz");
    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 0);
    assert_eq!(report["schema"], REPORT_SCHEMA);
    assert_eq!(report["profile"], PROFILE_V0);
    assert_ne!(report["profile"], PROFILE_V1);
}

#[test]
fn explicit_v1_never_falls_back_to_v0() {
    let bundle = v1_corpus().join("vectors/ok-002-deny-observation-missing.bundle.tar.gz");
    let (report, exit) = verify(&bundle, Some("v1"));
    assert_eq!(exit, 0);
    assert_eq!(report["schema"], REPORT_SCHEMA);
    assert_eq!(report["profile"], PROFILE_V1);
}

#[test]
fn v1_corpus_matches_manifest_under_explicit_v1() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(v1_corpus().join("MANIFEST.json")).expect("v1 MANIFEST"),
    )
    .expect("parse v1 MANIFEST");
    let vectors = manifest["vectors"].as_array().expect("vectors");
    assert_eq!(vectors.len(), 7, "focused v1 corpus");

    for vector in vectors {
        let id = vector["id"].as_str().expect("id");
        let file = v1_corpus().join(vector["file"].as_str().expect("file"));
        let expected = &vector["expected"];
        let (report, exit) = verify(&file, Some("v1"));

        assert_eq!(report["schema"], REPORT_SCHEMA, "{id}");
        assert_eq!(report["profile"], PROFILE_V1, "{id}: explicit v1");
        assert_eq!(
            report["bundle_integrity"], expected["bundle_integrity"],
            "{id}: integrity"
        );
        if expected["bundle_integrity"] == "fail" {
            assert!(report.get("verdict").is_none(), "{id}");
            assert_eq!(exit, 2, "{id}");
            continue;
        }
        assert_eq!(report["verdict"], expected["verdict"], "{id}: verdict");
        if expected["verdict"] == "valid" {
            assert_eq!(&report["claims"], &expected["claims"], "{id}: claims");
            assert_eq!(exit, 0, "{id}");
        } else {
            assert!(report.get("claims").is_none(), "{id}");
            assert_eq!(exit, 2, "{id}");
        }
    }
}

#[test]
fn default_v0_rejects_v1_observation_bundle() {
    let bundle = v1_corpus().join("vectors/ok-001-deny-bound-v1-observation.bundle.tar.gz");
    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 2);
    assert_eq!(report["profile"], PROFILE_V0);
    assert_eq!(report["verdict"], "invalid");
    assert!(report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|f| f["id"] == "unknown_profile_schema"));
}
