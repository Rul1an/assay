//! The #1975 journey as observed by a caller that reads only stdout and the exit status.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn workspace_root() -> &'static Path {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("assay-cli must live two components below the workspace root");
    assert!(
        root.join("Cargo.toml").is_file(),
        "workspace root does not contain Cargo.toml: {}",
        root.display()
    );
    root
}

fn contract() -> Value {
    let path = workspace_root().join("docs/generated/agent-golden-path.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read generated agent golden-path contract {}: {error}",
            path.display()
        )
    });
    let contract: Value = serde_json::from_str(&raw).expect("agent golden-path contract is JSON");
    assert_eq!(contract["schema"], "assay.agent_golden_path.v1");
    assert_eq!(contract["schema_version"], 1);
    contract
}

fn expected_outcome(step_id: &str, outcome_name: &str) -> Value {
    let contract = contract();
    let step = contract["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .find(|step| step["id"] == step_id)
        .unwrap_or_else(|| panic!("contract step {step_id:?} is missing"));
    let mut outcome = step["outcomes"]
        .as_array()
        .expect("step outcomes array")
        .iter()
        .find(|outcome| outcome["name"] == outcome_name)
        .unwrap_or_else(|| panic!("contract outcome {step_id}/{outcome_name} is missing"))
        .clone();
    outcome["command"] = step["command"].clone();
    outcome
}

fn assert_exit(output: &Output, expected: &Value, context: &str) {
    let expected_exit = expected["exit_code"]
        .as_i64()
        .expect("contract exit_code must be an integer");
    let actual_exit = output
        .status
        .code()
        .map(i64::from)
        .expect("assay process terminated without an exit code");
    assert_eq!(
        actual_exit,
        expected_exit,
        "{context} exit differed; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_gap(expected: &Value, issue: u64) {
    assert_eq!(
        expected["gap_issue"].as_u64(),
        Some(issue),
        "the measured gap must stay linked to its owning issue"
    );
}

fn assert_command(expected: &Value, command: &str) {
    assert_eq!(
        expected["command"], command,
        "the driven invocation drifted from the machine contract"
    );
}

fn assert_no_diagnosis(expected: &Value, document: &Value) {
    assert!(expected["reason_code"].is_null());
    assert!(expected["next_step"].is_null());
    assert!(document.get("reason_code").is_none());
    assert!(document.get("next_step").is_none());
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

fn assert_document(document: &Value, expected: &Value, context: &str) {
    let identity = expected["stdout"]["document"].as_str();
    if let Some(identity) = identity {
        assert_eq!(document["schema"], identity, "{context} document identity");
    } else if expected["stdout"]["kind"] == "json" {
        assert!(
            document.get("schema").is_none(),
            "{context} unexpectedly exposed an unpinned schema identity"
        );
    }
}

enum CorpusVector {
    Valid,
    Tampered,
}

fn corpus_vector(vector: CorpusVector) -> std::path::PathBuf {
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
    let expected = expected_outcome("install-check", "success");
    assert_command(&expected, "assay version");
    assert_exit(&output, &expected, "version");
    let version = String::from_utf8(output.stdout).expect("version stdout is UTF-8");
    let components: Vec<_> = version.trim().split('.').collect();
    assert_eq!(components.len(), 3, "version is not semver: {version:?}");
    assert!(
        components
            .iter()
            .all(|component| component.parse::<u64>().is_ok()),
        "version is not numeric semver: {version:?}"
    );
    assert_eq!(expected["stdout"]["kind"], "text");
}

#[test]
fn doctor_json_exposes_its_current_success_and_failure_surface() {
    let dir = tempfile::tempdir().expect("tempdir");
    let success = assay(dir.path(), &["doctor", "--format", "json"]);
    let expected_success = expected_outcome("preflight", "success");
    assert_command(&expected_success, "assay doctor --format json");
    assert_exit(&success, &expected_success, "doctor success");
    let success_json = stdout_json(&success, "doctor success");
    assert_document(&success_json, &expected_success, "doctor success");

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
    let expected_failure = expected_outcome("preflight", "invalid-config");
    assert_exit(&failure, &expected_failure, "doctor failure");
    let failure_json = stdout_json(&failure, "doctor failure");
    assert_document(&failure_json, &expected_failure, "doctor failure");
    assert_eq!(
        failure_json["config_error"]["code"],
        expected_failure["config_error_code"]
    );
    assert_no_diagnosis(&expected_failure, &failure_json);
    assert_gap(&expected_failure, 2160);
}

#[test]
fn init_stdout_records_success_but_not_the_failure_diagnosis() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let success = assay(
        success_dir.path(),
        &["init", "--preset", "dev", "--hello-trace"],
    );
    let expected_success = expected_outcome("starter-files", "success");
    assert_command(&expected_success, "assay init --preset dev --hello-trace");
    assert_exit(&success, &expected_success, "init success");
    let success_stdout = String::from_utf8(success.stdout).expect("init stdout is UTF-8");
    assert!(success_stdout.contains("Next: assay validate"));
    assert!(success_dir.path().join("policy.yaml").is_file());
    assert!(success_dir.path().join("eval.yaml").is_file());
    assert!(success_dir.path().join("traces/hello.jsonl").is_file());

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let failure = assay(failure_dir.path(), &["init", "--preset", "not-a-preset"]);
    let expected_failure = expected_outcome("starter-files", "unknown-preset");
    assert_exit(&failure, &expected_failure, "init failure");
    let failure_stdout = String::from_utf8(failure.stdout).expect("init stdout is UTF-8");
    assert!(
        !failure_stdout.contains("unknown preset"),
        "the actionable diagnosis unexpectedly moved to stdout; update the contract"
    );
    assert_gap(&expected_failure, 2161);
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
    let expected_success = expected_outcome("policy-validation", "valid");
    assert_command(
        &expected_success,
        "assay policy validate --input policy.yaml",
    );
    assert_exit(&success, &expected_success, "policy validation success");
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
    let expected_failure = expected_outcome("policy-validation", "malformed");
    assert_exit(&failure, &expected_failure, "policy validation failure");
    assert!(failure.stdout.is_empty());
    assert_gap(&expected_failure, 2162);
}

#[test]
fn completed_test_failure_is_a_run_report_not_a_diagnosis() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let init = assay(
        success_dir.path(),
        &["init", "--preset", "dev", "--hello-trace"],
    );
    assert_eq!(init.status.code(), Some(0));
    let success = assay(
        success_dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "traces/hello.jsonl",
            "--format",
            "json",
        ],
    );
    let expected_success = expected_outcome("evaluation-result", "success");
    assert_command(
        &expected_success,
        "assay run --config eval.yaml --format json",
    );
    assert_exit(&success, &expected_success, "completed run success");
    let success_json = stdout_json(&success, "completed run success");
    assert_document(&success_json, &expected_success, "completed run success");

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let failing_suite = workspace_root().join("tests/fixtures/contract/fail.yaml");
    let failure = assay(
        failure_dir.path(),
        &[
            "run",
            "--config",
            failing_suite.to_str().expect("UTF-8 path"),
            "--format",
            "json",
        ],
    );
    let expected_failure = expected_outcome("evaluation-result", "completed-test-failure");
    assert_exit(&failure, &expected_failure, "completed test failure");
    let failure_json = stdout_json(&failure, "completed test failure");
    assert_document(&failure_json, &expected_failure, "completed test failure");
    assert_eq!(failure_json["results"][0]["status"], "fail");
    assert_no_diagnosis(&expected_failure, &failure_json);
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
    let expected_success = expected_outcome("evidence-inspection", "valid");
    assert_command(
        &expected_success,
        "assay evidence show <bundle> --format json",
    );
    assert_exit(&success, &expected_success, "evidence show success");
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
    let expected_failure = expected_outcome("evidence-inspection", "tampered");
    assert_exit(&failure, &expected_failure, "evidence show failure");
    assert!(failure.stdout.is_empty());
    assert_gap(&expected_failure, 2164);
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
    let expected_success = expected_outcome("offline-profile-verification", "valid");
    assert_command(
        &expected_success,
        "assay evidence verify-privileged-mcp-action <bundle> --format json",
    );
    assert_exit(&success, &expected_success, "profile verify success");
    let success_json = stdout_json(&success, "profile verify success");
    assert_document(&success_json, &expected_success, "profile verify success");
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
    let expected_failure = expected_outcome("offline-profile-verification", "tampered");
    assert_exit(&failure, &expected_failure, "profile verify failure");
    let failure_json = stdout_json(&failure, "profile verify failure");
    assert_document(&failure_json, &expected_failure, "profile verify failure");
    assert_eq!(failure_json["bundle_integrity"], "fail");
    let findings = failure_json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1, "integrity failure must stay bounded");
    assert_eq!(findings[0]["id"], "bundle_integrity");
    assert!(failure_json.get("verdict").is_none());
    assert_no_diagnosis(&expected_failure, &failure_json);
    assert_gap(&expected_failure, 2165);
}
