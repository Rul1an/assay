//! The #1975 journey as observed by a caller that reads only stdout and the exit status.

#[path = "../../../tests/support/bounded_process.rs"]
mod bounded_process;
#[path = "../../../tests/support/agent_golden_path.rs"]
mod runtime_coverage;

use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use runtime_coverage::ExpectedOutcome;

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

fn expected_outcome(step_id: &str, outcome_name: &str) -> ExpectedOutcome {
    runtime_coverage::expected_outcome(&contract(), step_id, outcome_name)
}

#[test]
fn cli_contract_steps_default_to_invocation_cwd() {
    for step in contract()["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .filter(|step| step["binary"] == "assay")
    {
        assert_eq!(
            runtime_coverage::classify_working_directory(step),
            Ok(runtime_coverage::WorkingDirectory::Invocation),
            "CLI step {} unexpectedly depends on a source-repo cwd",
            step["id"]
        );
    }
}

fn assert_exit(output: &Output, expected: &ExpectedOutcome, context: &str) {
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
    runtime_coverage::record_observation(expected);
}

#[test]
fn golden_path_contract_uses_only_supported_binaries() {
    runtime_coverage::assert_contract_binaries(&contract(), &["assay", "assay-mcp-server"]);
}

fn assert_gap(expected: &Value, issue: u64) {
    assert_eq!(
        expected["gap_issue"].as_u64(),
        Some(issue),
        "the measured gap must stay linked to its owning issue"
    );
}

fn contract_argv(expected: &Value, replacements: &[(&str, &str)]) -> Vec<String> {
    let argv = expected["argv"]
        .as_array()
        .expect("contract outcome argv array");
    for (placeholder, _) in replacements {
        assert!(
            argv.iter().any(|argument| argument == *placeholder),
            "replacement {placeholder:?} is not present in contract argv"
        );
    }
    argv.iter()
        .map(|argument| {
            let argument = argument.as_str().expect("contract argv string");
            if argument.starts_with('<') && argument.ends_with('>') {
                replacements
                    .iter()
                    .find_map(|(placeholder, value)| (*placeholder == argument).then_some(*value))
                    .unwrap_or_else(|| {
                        panic!("contract argv placeholder {argument:?} is unresolved")
                    })
                    .to_string()
            } else {
                argument.to_string()
            }
        })
        .collect()
}

fn assert_no_diagnosis(expected: &Value, document: &Value) {
    assert!(expected["reason_code"].is_null());
    assert!(expected["next_step"].is_null());
    assert!(document.get("reason_code").is_none());
    assert!(document.get("next_step").is_none());
}

fn assay<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Output {
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
    command.current_dir(cwd).env("NO_COLOR", "1").args(args);
    run_bounded(
        command,
        b"",
        GOLDEN_PATH_LIMITS,
        "agent golden-path CLI command",
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

fn assay_contract(cwd: &Path, expected: &Value, replacements: &[(&str, &str)]) -> Output {
    assert_eq!(expected["binary"], "assay");
    let argv = contract_argv(expected, replacements);
    assay(cwd, &argv)
}

fn assert_stdout_kind(expected: &Value, kind: &str) {
    assert_eq!(
        expected["stdout"]["kind"], kind,
        "the observed stdout path drifted from the contract kind"
    );
}

fn stdout_text(output: &Output, expected: &Value, context: &str) -> String {
    assert_stdout_kind(expected, "text");
    assert!(expected["stdout"]["document"].is_null());
    String::from_utf8(output.stdout.clone())
        .unwrap_or_else(|error| panic!("{context} stdout is not UTF-8: {error}"))
}

fn assert_empty_stdout(output: &Output, expected: &Value, context: &str) {
    assert_stdout_kind(expected, "empty");
    assert!(expected["stdout"]["document"].is_null());
    assert!(output.stdout.is_empty(), "{context} stdout is not empty");
}

fn stdout_json(output: &Output, expected: &Value, context: &str) -> Value {
    assert_stdout_kind(expected, "json");
    let document = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context} stdout is not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_document(&document, expected, context);
    document
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
    let expected = expected_outcome("install-check", "success");
    let output = assay_contract(dir.path(), &expected, &[]);
    assert_exit(&output, &expected, "version");
    let version = stdout_text(&output, &expected, "version");
    let components: Vec<_> = version.trim().split('.').collect();
    assert_eq!(components.len(), 3, "version is not semver: {version:?}");
    assert!(
        components
            .iter()
            .all(|component| component.parse::<u64>().is_ok()),
        "version is not numeric semver: {version:?}"
    );
}

#[test]
fn doctor_json_failure_publishes_the_registered_reason_and_next_step() {
    let dir = tempfile::tempdir().expect("tempdir");
    let expected_success = expected_outcome("preflight", "success");
    let success = assay_contract(dir.path(), &expected_success, &[]);
    assert_exit(&success, &expected_success, "doctor success");
    stdout_json(&success, &expected_success, "doctor success");

    let missing = dir.path().join("missing.yaml");
    let expected_failure = expected_outcome("preflight", "invalid-config");
    let failure = assay_contract(
        dir.path(),
        &expected_failure,
        &[("<config>", missing.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "doctor failure");
    let failure_json = stdout_json(&failure, &expected_failure, "doctor failure");
    assert_eq!(
        failure_json["config_error"]["code"],
        expected_failure["config_error_code"]
    );
    // Read the contract side as a string rather than comparing two Values. Comparing Values lets a
    // coordinated regression pass: if the generator drops the field back to null and the binary
    // stops emitting it, `Null == Null` holds and the assertion is satisfied by two absences.
    let expected_reason = expected_failure["reason_code"]
        .as_str()
        .expect("contract reason_code string");
    assert_eq!(
        failure_json["reason_code"], expected_reason,
        "doctor config failure reason must match the generated contract"
    );
    let expected_next = expected_failure["next_step"]
        .as_str()
        .expect("contract next_step string")
        .replace("<config>", missing.to_str().expect("UTF-8 path"));
    assert_eq!(failure_json["next_step"], expected_next);
    assert!(
        expected_failure["gap_issue"].is_null(),
        "the doctor diagnosis gap is closed and must no longer carry an owning issue"
    );
}

#[test]
fn init_text_stays_the_human_progress_stream_it_has_always_been() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let expected_success = expected_outcome("starter-files", "success");
    let success = assay_contract(success_dir.path(), &expected_success, &[]);
    assert_exit(&success, &expected_success, "init success");
    let success_stdout = stdout_text(&success, &expected_success, "init success");
    assert!(success_stdout.contains("Next: assay validate"));
    assert!(success_dir.path().join("policy.yaml").is_file());
    assert!(success_dir.path().join("eval.yaml").is_file());
    assert!(success_dir.path().join("traces/hello.jsonl").is_file());

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let expected_failure = expected_outcome("starter-files", "unknown-preset");
    let failure = assay_contract(failure_dir.path(), &expected_failure, &[]);
    assert_exit(&failure, &expected_failure, "init failure");
    let failure_stdout = stdout_text(&failure, &expected_failure, "init failure");
    assert!(
        !failure_stdout.contains("unknown preset"),
        "the actionable diagnosis unexpectedly moved to the default stdout; update the contract"
    );
}

#[test]
fn init_json_publishes_the_registered_reason_and_next_step() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let expected_success = expected_outcome("starter-files", "success-json");
    let success = assay_contract(success_dir.path(), &expected_success, &[]);
    assert_exit(&success, &expected_success, "init json success");
    let success_json = stdout_json(&success, &expected_success, "init json success");
    let expected_success_reason = expected_success["reason_code"]
        .as_str()
        .expect("contract reason_code string");
    assert_eq!(success_json["reason_code"], expected_success_reason);

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let expected_failure = expected_outcome("starter-files", "unknown-preset-json");
    let failure = assay_contract(failure_dir.path(), &expected_failure, &[]);
    assert_exit(&failure, &expected_failure, "init json failure");
    let failure_json = stdout_json(&failure, &expected_failure, "init json failure");
    // Read each contract side as a string rather than comparing two Values: comparing Values lets
    // a coordinated regression pass, because two absences are equal to each other.
    let expected_reason = expected_failure["reason_code"]
        .as_str()
        .expect("contract reason_code string");
    assert_eq!(
        failure_json["reason_code"], expected_reason,
        "init failure reason must match the generated contract"
    );
    let expected_next = expected_failure["next_step"]
        .as_str()
        .expect("contract next_step string");
    assert_eq!(failure_json["next_step"], expected_next);
    assert!(
        expected_failure["gap_issue"].is_null(),
        "the init diagnosis gap is closed and must no longer carry an owning issue"
    );
}

#[test]
fn policy_validation_json_carries_success_and_failure_contracts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let init = assay(dir.path(), &["init", "--preset", "dev", "--hello-trace"]);
    assert_eq!(init.status.code(), Some(0));

    let valid_policy = dir.path().join("policy.yaml");
    let expected_success = expected_outcome("policy-validation", "valid");
    let success = assay_contract(
        dir.path(),
        &expected_success,
        &[("<policy>", valid_policy.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&success, &expected_success, "policy validation success");
    let success_json = stdout_json(&success, &expected_success, "policy validation success");
    assert_eq!(success_json["exit_code"], 0);
    assert_eq!(
        success_json["reason_code"], expected_success["reason_code"],
        "valid policy reason must match the generated contract"
    );
    assert!(success_json.get("next_step").is_none());

    let malformed_policy = dir.path().join("malformed.yaml");
    std::fs::write(&malformed_policy, "version: [\n").expect("write malformed policy");
    let expected_failure = expected_outcome("policy-validation", "malformed");
    let failure = assay_contract(
        dir.path(),
        &expected_failure,
        &[("<policy>", malformed_policy.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "policy validation failure");
    let failure_json = stdout_json(&failure, &expected_failure, "policy validation failure");
    assert_eq!(
        failure_json["reason_code"], expected_failure["reason_code"],
        "malformed policy reason must match the generated contract"
    );
    let expected_next = expected_failure["next_step"]
        .as_str()
        .expect("contract next_step string")
        .replace("<policy>", malformed_policy.to_str().expect("UTF-8 path"));
    assert_eq!(failure_json["next_step"], expected_next);
}

#[test]
fn completed_test_failure_is_a_run_report_not_a_diagnosis() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let init = assay(
        success_dir.path(),
        &["init", "--preset", "dev", "--hello-trace"],
    );
    assert_eq!(init.status.code(), Some(0));
    let expected_success = expected_outcome("evaluation-result", "success");
    let success = assay_contract(success_dir.path(), &expected_success, &[]);
    assert_exit(&success, &expected_success, "completed run success");
    stdout_json(&success, &expected_success, "completed run success");

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let failing_suite = workspace_root().join("tests/fixtures/contract/fail.yaml");
    let expected_failure = expected_outcome("evaluation-result", "completed-test-failure");
    let failure = assay_contract(
        failure_dir.path(),
        &expected_failure,
        &[("<config>", failing_suite.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "completed test failure");
    let failure_json = stdout_json(&failure, &expected_failure, "completed test failure");
    assert_eq!(failure_json["results"][0]["status"], "fail");
    assert_no_diagnosis(&expected_failure, &failure_json);
}

#[test]
fn bundle_inspection_json_disappears_on_integrity_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let valid = corpus_vector(CorpusVector::Valid);
    let expected_success = expected_outcome("evidence-inspection", "valid");
    let success = assay_contract(
        dir.path(),
        &expected_success,
        &[("<bundle>", valid.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&success, &expected_success, "evidence show success");
    let success_json = stdout_json(&success, &expected_success, "evidence show success");
    assert_eq!(success_json["manifest"]["event_count"], 2);
    assert!(success_json["events"].is_array());

    let tampered = corpus_vector(CorpusVector::Tampered);
    let expected_failure = expected_outcome("evidence-inspection", "tampered");
    let failure = assay_contract(
        dir.path(),
        &expected_failure,
        &[("<bundle>", tampered.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "evidence show failure");
    assert_empty_stdout(&failure, &expected_failure, "evidence show failure");
    assert_gap(&expected_failure, 2164);
}

#[test]
fn offline_profile_verifier_keeps_both_outcomes_on_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let valid = corpus_vector(CorpusVector::Valid);
    let expected_success = expected_outcome("offline-profile-verification", "valid");
    let success = assay_contract(
        dir.path(),
        &expected_success,
        &[("<bundle>", valid.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&success, &expected_success, "profile verify success");
    let success_json = stdout_json(&success, &expected_success, "profile verify success");
    assert_eq!(success_json["bundle_integrity"], "pass");
    assert_eq!(success_json["verdict"], "valid");

    let tampered = corpus_vector(CorpusVector::Tampered);
    let expected_failure = expected_outcome("offline-profile-verification", "tampered");
    let failure = assay_contract(
        dir.path(),
        &expected_failure,
        &[("<bundle>", tampered.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "profile verify failure");
    let failure_json = stdout_json(&failure, &expected_failure, "profile verify failure");
    assert_eq!(failure_json["bundle_integrity"], "fail");
    let findings = failure_json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1, "integrity failure must stay bounded");
    assert_eq!(findings[0]["id"], "bundle_integrity");
    assert!(failure_json.get("verdict").is_none());
    assert_no_diagnosis(&expected_failure, &failure_json);
    assert_gap(&expected_failure, 2165);
}

#[test]
fn every_cli_contract_outcome_is_executed_once() {
    runtime_coverage::assert_exact(
        &contract(),
        "assay",
        &[
            installed_binary_reports_a_version_on_stdout,
            doctor_json_failure_publishes_the_registered_reason_and_next_step,
            init_text_stays_the_human_progress_stream_it_has_always_been,
            init_json_publishes_the_registered_reason_and_next_step,
            policy_validation_json_carries_success_and_failure_contracts,
            completed_test_failure_is_a_run_report_not_a_diagnosis,
            bundle_inspection_json_disappears_on_integrity_failure,
            offline_profile_verifier_keeps_both_outcomes_on_stdout,
        ],
    );
}
