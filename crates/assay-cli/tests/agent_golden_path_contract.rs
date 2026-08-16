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

fn invalid_contract_bundle() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/evidence/invalid-manifest.bundle.tar.gz")
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

/// A config that loads and produces no diagnostics, so the success row is measured on a preflight
/// that actually examined something.
const CLEAN_EVAL_YAML: &str = r#"configVersion: 1
suite: "preflight_success"
model: "trace"
tests:
  - id: "preflight_success_regex"
    input:
      prompt: "hello_prompt"
    expected:
      type: regex_match
      pattern: "Hello\\s+Assay"
      flags: ["i"]
"#;

#[test]
fn doctor_json_failure_publishes_the_registered_reason_and_next_step() {
    let dir = tempfile::tempdir().expect("tempdir");

    // The success row is measured on a config that was actually examined. It was measured on an
    // empty directory once, which published `Success 0` for a run in which no config validation
    // occurred (#2210). Both rows are exit 0, so `config_check.status` is the only thing that
    // separates them, and the two are driven here rather than described.
    let clean_config = dir.path().join("clean.yaml");
    std::fs::write(&clean_config, CLEAN_EVAL_YAML).expect("wrote clean config");
    let expected_success = expected_outcome("preflight", "success");
    let success = assay_contract(
        dir.path(),
        &expected_success,
        &[("<config>", clean_config.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&success, &expected_success, "doctor success");
    let success_json = stdout_json(&success, &expected_success, "doctor success");
    assert_eq!(
        success_json["config_check"]["status"], expected_success["config_check"],
        "the success row claims a config was examined"
    );

    let empty = tempfile::tempdir().expect("tempdir");
    let expected_skipped = expected_outcome("preflight", "no-config");
    let skipped = assay_contract(empty.path(), &expected_skipped, &[]);
    assert_exit(&skipped, &expected_skipped, "doctor no-config");
    let skipped_json = stdout_json(&skipped, &expected_skipped, "doctor no-config");
    assert_eq!(
        skipped_json["config_check"]["status"], expected_skipped["config_check"],
        "an unexamined config is published as such rather than as a clean one"
    );
    assert_ne!(
        success_json["config_check"]["status"], skipped_json["config_check"]["status"],
        "both runs exit 0, so a consumer can only tell them apart by this key"
    );

    // The examined-and-failing row. Its exit is the one `decide_exit` gives this diagnostic, so
    // driving it here is what stops the guide from asserting a class nothing measures.
    let bad_trace_dir = tempfile::tempdir().expect("tempdir");
    let bad_config = bad_trace_dir.path().join("clean.yaml");
    std::fs::write(&bad_config, CLEAN_EVAL_YAML).expect("wrote config");
    let expected_diag = expected_outcome("preflight", "diagnostics-error");
    let diag = assay_contract(
        bad_trace_dir.path(),
        &expected_diag,
        &[
            ("<config>", bad_config.to_str().expect("UTF-8 path")),
            ("<trace>", "traces/absent.jsonl"),
        ],
    );
    assert_exit(&diag, &expected_diag, "doctor diagnostics-error");
    let diag_json = stdout_json(&diag, &expected_diag, "doctor diagnostics-error");
    assert_eq!(
        diag_json["config_check"]["status"], expected_diag["config_check"],
        "the config was examined; the error is in what it found"
    );
    assert!(
        diag_json["data_diagnostics"]
            .as_array()
            .expect("data_diagnostics is an array")
            .iter()
            .any(|d| d["severity"] == "error"),
        "this row exists to pin the exit class for an error-severity diagnostic"
    );

    let missing = dir.path().join("missing.yaml");
    let expected_missing = expected_outcome("preflight", "missing-config");
    let missing_run = assay_contract(
        dir.path(),
        &expected_missing,
        &[("<config>", missing.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&missing_run, &expected_missing, "doctor missing-config");
    let missing_json = stdout_json(&missing_run, &expected_missing, "doctor missing-config");
    assert_eq!(
        missing_json["config_error"]["code"],
        expected_missing["config_error_code"]
    );
    // Read the contract side as a string rather than comparing two Values. Comparing Values lets a
    // coordinated regression pass: if the generator drops the field back to null and the binary
    // stops emitting it, `Null == Null` holds and the assertion is satisfied by two absences.
    let expected_missing_reason = expected_missing["reason_code"]
        .as_str()
        .expect("contract reason_code string");
    assert_eq!(
        missing_json["reason_code"], expected_missing_reason,
        "a proven-absent explicit config must match the generated contract"
    );
    assert_eq!(
        missing_json["next_step"],
        expected_missing["next_step"]
            .as_str()
            .expect("contract next_step string"),
        "absent-config recovery is assay init, not a doctor self-loop"
    );
    assert!(
        expected_missing["gap_issue"].is_null(),
        "the doctor diagnosis gap is closed and must no longer carry an owning issue"
    );

    let malformed = dir.path().join("bad.yaml");
    std::fs::write(&malformed, "version: [\n").expect("wrote malformed config");
    let expected_invalid = expected_outcome("preflight", "invalid-config");
    let invalid = assay_contract(
        dir.path(),
        &expected_invalid,
        &[("<config>", malformed.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&invalid, &expected_invalid, "doctor invalid-config");
    let invalid_json = stdout_json(&invalid, &expected_invalid, "doctor invalid-config");
    assert_eq!(
        invalid_json["config_error"]["code"],
        expected_invalid["config_error_code"]
    );
    let expected_invalid_reason = expected_invalid["reason_code"]
        .as_str()
        .expect("contract reason_code string");
    assert_eq!(
        invalid_json["reason_code"], expected_invalid_reason,
        "a present malformed config must stay E_CFG_PARSE"
    );
    let expected_invalid_next = expected_invalid["next_step"]
        .as_str()
        .expect("contract next_step string")
        .replace("<config>", malformed.to_str().expect("UTF-8 path"));
    assert_eq!(invalid_json["next_step"], expected_invalid_next);
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
    // Read the contract side as a string for the same reason the failure side does below. This one
    // was `null` in the generated contract while the binary emitted a concrete argv, and nothing
    // compared them, so the published document understated what a success actually carries.
    let expected_success_next = expected_success["next_step"]
        .as_str()
        .expect("contract next_step string");
    assert_eq!(
        success_json["next_step"], expected_success_next,
        "init success next_step must match the generated contract"
    );

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
fn bundle_inspection_json_publishes_typed_failures_on_stdout() {
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
    assert_eq!(success_json["verify_mode"], "enabled");

    let expected_skipped = expected_outcome("evidence-inspection", "verification-disabled");
    let skipped = assay_contract(
        dir.path(),
        &expected_skipped,
        &[(
            "<bundle>",
            corpus_vector(CorpusVector::Tampered)
                .to_str()
                .expect("UTF-8 path"),
        )],
    );
    assert_exit(&skipped, &expected_skipped, "evidence show --no-verify");
    let skipped_json = stdout_json(&skipped, &expected_skipped, "evidence show --no-verify");
    assert_eq!(skipped_json["verify_mode"], "disabled");

    let missing_unverified = assay(
        dir.path(),
        &[
            "evidence",
            "show",
            "missing.bundle.tar.gz",
            "--format",
            "json",
            "--no-verify",
        ],
    );
    assert_eq!(missing_unverified.status.code(), Some(2));
    let missing_unverified_json: Value = serde_json::from_slice(&missing_unverified.stdout)
        .expect("unreadable --no-verify stdout must be JSON");
    assert_eq!(
        missing_unverified_json["provenance"]["verify_mode"],
        "disabled"
    );

    let tampered = corpus_vector(CorpusVector::Tampered);
    let expected_failure = expected_outcome("evidence-inspection", "tampered");
    let failure = assay_contract(
        dir.path(),
        &expected_failure,
        &[("<bundle>", tampered.to_str().expect("UTF-8 path"))],
    );
    assert_exit(&failure, &expected_failure, "evidence show failure");
    let failure_json = stdout_json(&failure, &expected_failure, "evidence show failure");
    assert_eq!(
        failure_json["reason_code"], expected_failure["reason_code"],
        "integrity failure must publish the registered contract reason"
    );
    assert_eq!(failure_json["exit_code"], 2);
    let next_step = failure_json["next_step"]
        .as_str()
        .expect("integrity failure next_step");
    assert_eq!(
        Some(next_step),
        expected_failure["next_step"].as_str(),
        "integrity remediation must match the generated contract"
    );
    assert!(!next_step.trim().is_empty());

    let missing = dir.path().join("missing bundle.tar.gz");
    let expected_unreadable = expected_outcome("evidence-inspection", "unreadable");
    let unreadable = assay_contract(
        dir.path(),
        &expected_unreadable,
        &[("<bundle>", missing.to_str().expect("UTF-8 path"))],
    );
    assert_exit(
        &unreadable,
        &expected_unreadable,
        "evidence show unreadable",
    );
    let unreadable_json = stdout_json(
        &unreadable,
        &expected_unreadable,
        "evidence show unreadable",
    );
    assert_eq!(unreadable_json["reason_code"], "E_EVIDENCE_UNREADABLE");
    assert_ne!(unreadable_json["reason_code"], "E_EVIDENCE_INTEGRITY");
    let expected_next_step = expected_unreadable["next_step"]
        .as_str()
        .expect("contract unreadable next_step")
        .replace("<bundle>", missing.to_str().expect("UTF-8 path"));
    assert_eq!(unreadable_json["next_step"], expected_next_step);
    let recovery = unreadable_json["next_step"]
        .as_str()
        .expect("unreadable bundle next_step")
        .strip_prefix("Run argv: ")
        .expect("unreadable bundle recovery must be JSON argv");
    let argv: Vec<String> = serde_json::from_str(recovery).expect("recovery argv must parse");
    assert_eq!(
        argv,
        [
            "assay",
            "evidence",
            "show",
            "--format",
            "json",
            "--",
            missing.to_str().expect("UTF-8 path"),
        ]
    );

    let invalid_contract = invalid_contract_bundle();
    let expected_contract_failure =
        expected_outcome("evidence-inspection", "format-contract-failure");
    let contract_failure = assay_contract(
        dir.path(),
        &expected_contract_failure,
        &[("<bundle>", invalid_contract.to_str().expect("UTF-8 path"))],
    );
    assert_exit(
        &contract_failure,
        &expected_contract_failure,
        "evidence show format-contract failure",
    );
    assert!(contract_failure.stdout.is_empty());
    assert!(String::from_utf8_lossy(&contract_failure.stderr).contains("ContractInvalidJson"));
    assert_gap(&expected_contract_failure, 2412);
}

#[test]
fn evidence_inspection_contract_discloses_the_deferred_format_contract_gap() {
    let contract = contract();
    let step = contract["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .find(|step| step["id"] == "evidence-inspection")
        .expect("evidence inspection step");
    let failure_summary = step["failure_summary"]
        .as_str()
        .expect("evidence inspection failure summary");
    assert!(
        failure_summary.contains("gap #2412"),
        "the residual format-contract failure must remain disclosed"
    );
    assert!(
        failure_summary.contains("empty stdout"),
        "the residual gap must state its machine-channel consequence"
    );
    let stdout_summary = step["stdout_summary"]
        .as_str()
        .expect("evidence inspection stdout summary");
    assert!(
        stdout_summary.contains("verify_mode")
            && stdout_summary.contains("enabled")
            && stdout_summary.contains("disabled"),
        "the shipped contract must publish the verification-mode field and vocabulary"
    );
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
    assert_eq!(
        failure_json["reason_code"], expected_failure["reason_code"],
        "tamper must publish the registered integrity reason"
    );
    assert_eq!(
        failure_json["next_step"], expected_failure["next_step"],
        "tamper remediation must match the generated contract"
    );
    assert!(expected_failure["gap_issue"].is_null());
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
            bundle_inspection_json_publishes_typed_failures_on_stdout,
            offline_profile_verifier_keeps_both_outcomes_on_stdout,
        ],
    );
}
