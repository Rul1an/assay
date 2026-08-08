//! The #1975 journey as observed by a caller that reads only stdout and the exit status.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn contract_guide() -> String {
    let path = workspace_root().join("docs/guides/agent-golden-path.md");
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read agent golden-path contract {}: {error}",
            path.display()
        )
    })
}

fn assert_documented(needles: &[&str]) {
    let guide = contract_guide();
    for needle in needles {
        assert!(
            guide.contains(needle),
            "agent golden-path contract does not pin {needle:?}"
        );
    }
}

fn assay(cwd: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run assay binary")
}

fn stdout_json(output: &Output, context: &str) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context} stdout is not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

enum CorpusVector {
    Valid,
    Tampered,
}

fn corpus_vector(vector: CorpusVector) -> PathBuf {
    let name = match vector {
        CorpusVector::Valid => "ok-001-deny-bound-observation.bundle.tar.gz",
        CorpusVector::Tampered => "bad-101-tampered-bundle.bundle.tar.gz",
    };
    workspace_root()
        .join("conformance/privileged-mcp-action-v0/vectors")
        .join(name)
}

#[test]
fn installed_binary_reports_a_version_on_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = assay(dir.path(), &["version"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "version failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = String::from_utf8(output.stdout).expect("version stdout is UTF-8");
    let components: Vec<_> = version.trim().split('.').collect();
    assert_eq!(components.len(), 3, "version is not semver: {version:?}");
    assert!(
        components
            .iter()
            .all(|component| component.parse::<u64>().is_ok()),
        "version is not numeric semver: {version:?}"
    );

    assert_documented(&["| 1. Install check |", "`assay version`"]);
}

#[test]
fn doctor_json_exposes_its_current_success_and_failure_surface() {
    let dir = tempfile::tempdir().expect("tempdir");
    let success = assay(dir.path(), &["doctor", "--format", "json"]);
    assert_eq!(success.status.code(), Some(0));
    let success_json = stdout_json(&success, "doctor success");
    assert_eq!(success_json["schema"], "assay.doctor_report.v0");

    let missing = dir.path().join("missing.yaml");
    let failure = assay(
        dir.path(),
        &[
            "doctor",
            "--format",
            "json",
            "--config",
            missing.to_str().expect("UTF-8 path"),
        ],
    );
    assert_eq!(failure.status.code(), Some(1));
    let failure_json = stdout_json(&failure, "doctor failure");
    assert_eq!(failure_json["schema"], "assay.doctor_report.v0");
    assert_eq!(failure_json["config_error"]["code"], "E_CFG_PARSE");
    assert!(failure_json.get("reason_code").is_none());
    assert!(failure_json.get("next_step").is_none());

    assert_documented(&["| 2. Preflight |", "`assay doctor --format json`", "#2160"]);
}

#[test]
fn init_stdout_records_success_but_not_the_failure_diagnosis() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let success = assay(
        success_dir.path(),
        &["init", "--preset", "dev", "--hello-trace"],
    );
    assert_eq!(
        success.status.code(),
        Some(0),
        "init failed: {}",
        String::from_utf8_lossy(&success.stderr)
    );
    let success_stdout = String::from_utf8(success.stdout).expect("init stdout is UTF-8");
    assert!(success_stdout.contains("Next: assay validate"));
    assert!(success_dir.path().join("policy.yaml").is_file());
    assert!(success_dir.path().join("eval.yaml").is_file());
    assert!(success_dir.path().join("traces/hello.jsonl").is_file());

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let failure = assay(failure_dir.path(), &["init", "--preset", "not-a-preset"]);
    assert_eq!(failure.status.code(), Some(2));
    let failure_stdout = String::from_utf8(failure.stdout).expect("init stdout is UTF-8");
    assert!(
        !failure_stdout.contains("unknown preset"),
        "the actionable diagnosis unexpectedly moved to stdout; update the contract"
    );

    assert_documented(&[
        "| 3. Starter files |",
        "`assay init --preset dev --hello-trace`",
        "#2161",
    ]);
}

#[test]
fn policy_validation_stdout_is_currently_empty_on_both_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let init = assay(dir.path(), &["init", "--preset", "dev", "--hello-trace"]);
    assert_eq!(init.status.code(), Some(0));

    let valid_policy = dir.path().join("policy.yaml");
    let success = assay(
        dir.path(),
        &[
            "policy",
            "validate",
            "--input",
            valid_policy.to_str().expect("UTF-8 path"),
        ],
    );
    assert_eq!(success.status.code(), Some(0));
    assert!(success.stdout.is_empty());

    let malformed_policy = dir.path().join("malformed.yaml");
    std::fs::write(&malformed_policy, "version: [\n").expect("write malformed policy");
    let failure = assay(
        dir.path(),
        &[
            "policy",
            "validate",
            "--input",
            malformed_policy.to_str().expect("UTF-8 path"),
        ],
    );
    assert_eq!(failure.status.code(), Some(2));
    assert!(failure.stdout.is_empty());

    assert_documented(&[
        "| 4. Policy validation |",
        "`assay policy validate --input policy.yaml`",
        "#2162",
    ]);
}

#[test]
fn bundle_inspection_json_disappears_on_integrity_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let valid = corpus_vector(CorpusVector::Valid);
    let success = assay(
        dir.path(),
        &[
            "evidence",
            "show",
            valid.to_str().expect("UTF-8 path"),
            "--format",
            "json",
        ],
    );
    assert_eq!(success.status.code(), Some(0));
    let success_json = stdout_json(&success, "evidence show success");
    assert_eq!(success_json["manifest"]["event_count"], 2);
    assert!(success_json["events"].is_array());

    let tampered = corpus_vector(CorpusVector::Tampered);
    let failure = assay(
        dir.path(),
        &[
            "evidence",
            "show",
            tampered.to_str().expect("UTF-8 path"),
            "--format",
            "json",
        ],
    );
    assert_eq!(failure.status.code(), Some(2));
    assert!(failure.stdout.is_empty());

    assert_documented(&[
        "| 6. Evidence inspection |",
        "`assay evidence show <bundle> --format json`",
        "#2164",
    ]);
}

#[test]
fn offline_profile_verifier_keeps_both_outcomes_on_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let valid = corpus_vector(CorpusVector::Valid);
    let success = assay(
        dir.path(),
        &[
            "evidence",
            "verify-privileged-mcp-action",
            valid.to_str().expect("UTF-8 path"),
            "--format",
            "json",
        ],
    );
    assert_eq!(success.status.code(), Some(0));
    let success_json = stdout_json(&success, "profile verify success");
    assert_eq!(
        success_json["schema"],
        "assay.privileged_mcp_action.verify.report.v0"
    );
    assert_eq!(success_json["bundle_integrity"], "pass");
    assert_eq!(success_json["verdict"], "valid");

    let tampered = corpus_vector(CorpusVector::Tampered);
    let failure = assay(
        dir.path(),
        &[
            "evidence",
            "verify-privileged-mcp-action",
            tampered.to_str().expect("UTF-8 path"),
            "--format",
            "json",
        ],
    );
    assert_eq!(failure.status.code(), Some(2));
    let failure_json = stdout_json(&failure, "profile verify failure");
    assert_eq!(
        failure_json["schema"],
        "assay.privileged_mcp_action.verify.report.v0"
    );
    assert_eq!(failure_json["bundle_integrity"], "fail");
    let findings = failure_json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1, "integrity failure must stay bounded");
    assert_eq!(findings[0]["id"], "bundle_integrity");
    assert!(failure_json.get("verdict").is_none());
    assert!(failure_json.get("reason_code").is_none());
    assert!(failure_json.get("next_step").is_none());

    assert_documented(&[
        "| 7. Offline profile verification |",
        "`assay evidence verify-privileged-mcp-action <bundle> --format json`",
        "assay.privileged_mcp_action.verify.report.v0",
        "#2165",
    ]);
}
