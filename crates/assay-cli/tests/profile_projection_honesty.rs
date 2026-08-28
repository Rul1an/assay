//! P1-A: `report.profile` is the selected interpreter, never carried input identity.
//!
//! Frozen v0/v1 bundles do not carry a profile id. Changing `--profile-version` may
//! change the selected interpreter and MUST NOT invent input-profile provenance.

use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_V0: &str = "privileged-mcp-action/v0";
const PROFILE_V1: &str = "privileged-mcp-action/v1";
const DECISION_ONLY_SHA256: &str =
    "97e93d3e7653e24e7e475cef37e8a4e8e3f2bc83b39158457132c60f61f09973";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn v1_corpus() -> PathBuf {
    repo_root().join("conformance/privileged-mcp-action-v1")
}

fn decision_only_bundle() -> PathBuf {
    v1_corpus().join("vectors/ok-002-deny-observation-missing.bundle.tar.gz")
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

fn assert_undeclared_legacy_input(report: &Value) {
    assert!(
        report["input_profile"].is_null(),
        "v0/v1 carry no profile id, got {}",
        report["input_profile"]
    );
    assert_eq!(report["input_profile_status"], "undeclared_legacy");
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

#[test]
fn decision_only_fixture_bytes_are_pinned() {
    let bytes = std::fs::read(decision_only_bundle()).expect("read decision-only fixture");
    assert_eq!(sha256_hex(&bytes), DECISION_ONLY_SHA256);
}

#[test]
fn report_always_projects_undeclared_legacy_input_identity() {
    for version in [None, Some("v0"), Some("v1")] {
        let (report, exit) = verify(&decision_only_bundle(), version);
        assert_eq!(exit, 0, "selector {version:?}");
        assert_eq!(report["schema"], REPORT_SCHEMA, "selector {version:?}");
        assert!(
            report.get("profile").is_some(),
            "profile remains for compatibility"
        );
        match version {
            None => assert_eq!(report["profile_selection"], "default"),
            Some(_) => assert_eq!(report["profile_selection"], "explicit"),
        }
        assert_undeclared_legacy_input(&report);
    }
}

#[test]
fn identical_bytes_change_selected_profile_not_input_identity() {
    let (v0, e0) = verify(&decision_only_bundle(), Some("v0"));
    let (v1, e1) = verify(&decision_only_bundle(), Some("v1"));
    assert_eq!(e0, 0);
    assert_eq!(e1, 0);
    assert_eq!(v0["profile"], PROFILE_V0);
    assert_eq!(v1["profile"], PROFILE_V1);
    assert_ne!(v0["profile"], v1["profile"]);
    assert_eq!(v0["profile_selection"], "explicit");
    assert_eq!(v1["profile_selection"], "explicit");
    assert_eq!(v0["input_profile"], v1["input_profile"]);
    assert_eq!(v0["input_profile_status"], v1["input_profile_status"]);
    assert_undeclared_legacy_input(&v0);
    assert_undeclared_legacy_input(&v1);
    assert_eq!(v0["verdict"], v1["verdict"]);
    assert_eq!(v0["claims"], v1["claims"]);
}

#[test]
fn default_v0_versus_explicit_v0_distinguishes_selection_not_verdict() {
    let (default, ed) = verify(&decision_only_bundle(), None);
    let (explicit, ee) = verify(&decision_only_bundle(), Some("v0"));
    assert_eq!(ed, 0);
    assert_eq!(ee, 0);
    assert_eq!(default["profile"], PROFILE_V0);
    assert_eq!(explicit["profile"], PROFILE_V0);
    assert_eq!(default["profile_selection"], "default");
    assert_eq!(explicit["profile_selection"], "explicit");
    assert_ne!(default["profile_selection"], explicit["profile_selection"]);
    assert_eq!(default["verdict"], explicit["verdict"]);
    assert_eq!(default["claims"], explicit["claims"]);
    assert_undeclared_legacy_input(&default);
    assert_undeclared_legacy_input(&explicit);
}

#[test]
fn unknown_in_namespace_schema_is_retained_on_the_finding() {
    let bundle = v1_corpus().join("vectors/ok-001-deny-bound-v1-observation.bundle.tar.gz");
    let (report, exit) = verify(&bundle, None);
    assert_eq!(exit, 2);
    assert!(report.get("claims").is_none(), "claims must stay absent");
    let finding = report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["id"] == "unknown_profile_schema")
        .expect("unknown_profile_schema finding");
    assert_eq!(
        finding["observed_schema"],
        "assay.denied_call_observation.v1"
    );
    assert!(
        finding.get("observed_schema").is_some(),
        "exact schema must be a finding field, not only prose"
    );
}
