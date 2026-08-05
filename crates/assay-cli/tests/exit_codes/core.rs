use super::*;

#[test]
fn contract_ci_report_io_failure() {
    let dir = tempdir().unwrap();
    // Valid config, 1 passing test
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .unwrap();

    // Output is a directory -> IO Error
    let bad_path = dir.path().join("bad_output");
    fs::create_dir(&bad_path).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("ci")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--junit")
        .arg(&bad_path)
        .arg("--sarif")
        .arg(&bad_path)
        .assert()
        .success(); // Option B: Success

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 0);

    // Verification of Machine-Readable Warnings (Strict)
    let warnings = v
        .get("warnings")
        .expect("warnings field missing in run.json")
        .as_array()
        .expect("warnings must be an array");

    // Expect exactly 2 warnings (JUnit and SARIF)
    assert_eq!(
        warnings.len(),
        2,
        "Expected exactly 2 warnings (JUnit + SARIF)"
    );

    let has_junit = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write JUnit"));
    let has_sarif = warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Failed to write SARIF"));

    assert!(
        has_junit,
        "Missing JUnit warning in run.json. Found: {:?}",
        warnings
    );
    assert!(
        has_sarif,
        "Missing SARIF warning in run.json. Found: {:?}",
        warnings
    );
}

#[test]
fn contract_run_json_always_written_arg_conflict() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--baseline")
        .arg("dummy")
        .arg("--export-baseline")
        .arg("dummy")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_INVALID_ARGS");
}

#[test]
fn contract_model_trace_requires_trace_file() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: trace-requires-input
model: trace
tests:
  - id: t1
    input: { prompt: "hello" }
    expected: { type: must_contain, must_contain: ["hello"] }
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2)
        // The console text is now the classified message itself rather than a separately
        // worded copy of it, so this is the same string the artifact carries.
        .stderr(predicate::str::contains("E_INVALID_ARGS"))
        .stderr(predicate::str::contains(
            "config uses model: trace, so --trace-file <PATH> is required",
        ));

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_INVALID_ARGS");
    assert!(v["resolution"]["message"]
        .as_str()
        .expect("resolution.message must be a string")
        .contains("--trace-file <PATH> is required"));
    assert_run_json_seeds_early_exit(&v);
}

#[test]
fn contract_reason_code_trace_not_found_v2() {
    let dir = tempdir().unwrap();
    // Valid config schema with ID
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
    assert_run_json_seeds_early_exit(&v);
    let summary = read_summary_json(dir.path());
    assert_summary_seeds_early_exit(&summary);
}

#[test]
fn contract_legacy_v1_trace_not_found() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: dummy\n    input: hello",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v1")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("missing_trace.jsonl")
        .assert()
        .code(3);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 3);
    assert_eq!(v["reason_code"], "E_TRACE_NOT_FOUND");
}

/// E7.2: Happy path — run completes; run.json and summary.json contain seed_version 1 and integer order_seed/judge_seed.
#[test]
fn contract_e72_seeds_happy_path() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"version: 1
suite: e72-seeds
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected: { type: must_contain, must_contain: ["passed"] }
"#,
    )
    .unwrap();
    // Minimal v2 trace: episode_start + episode_end for t1 with final_output containing "passed"
    fs::write(
        dir.path().join("trace.jsonl"),
        r#"{"type":"episode_start","episode_id":"t1","timestamp":1000,"input":{"prompt":"hi"}}
{"type":"episode_end","episode_id":"t1","timestamp":2000,"final_output":"passed"}
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--trace-file")
        .arg("trace.jsonl")
        .arg("--strict")
        .assert()
        .success();

    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 0);
    assert_run_json_seeds_happy(&run);
    let summary = read_summary_json(dir.path());
    assert_summary_seeds_happy(&summary);
}

#[test]
fn contract_exit_codes_missing_config() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("non_existent.yaml")
        .assert()
        .code(2);

    let v = read_run_json(dir.path());
    assert_schema(&v);
    assert_eq!(v["exit_code"], 2);
}

#[test]
fn contract_run_deny_deprecations_fails_on_legacy_policy_usage() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("eval.yaml"),
        r#"configVersion: 1
suite: strict-deprecations
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
      type: args_valid
      policy: policy.yaml
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("eval.yaml")
        .arg("--deny-deprecations")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_ci_deny_deprecations_fails_on_legacy_policy_usage() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("eval.yaml"),
        r#"configVersion: 1
suite: strict-deprecations-ci
model: dummy
tests:
  - id: t1
    input: { prompt: "hi" }
    expected:
      type: args_valid
      policy: policy.yaml
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("ci")
        .arg("--config")
        .arg("eval.yaml")
        .arg("--deny-deprecations")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_run_preflight_contract_error_writes_cfg_parse_artifact() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("eval.yaml"),
        r#"configVersion: 1
suite: preflight-contract
model: dummy
settings:
  cache: false
tests:
  - id: invalid-static-input
    input: { prompt: "hi" }
    expected:
      type: json_schema
      json_schema: ""
      schema_file: missing.schema.json
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("eval.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
    assert!(
        run["resolution"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing.schema.json")),
        "{run:#}"
    );
}

#[test]
fn contract_run_format_json_emits_report_to_stdout() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .unwrap();

    let out = Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .clone();

    // stdout carries a valid, parseable JSON report (the CI piping contract).
    let stdout = String::from_utf8(out.stdout).expect("stdout utf8");
    let report: Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON for --format json");
    assert!(report.get("run_id").is_some(), "report missing run_id");
    assert!(report.get("suite").is_some(), "report missing suite");
    assert!(
        report.get("results").and_then(|r| r.as_array()).is_some(),
        "report missing results array"
    );

    // The run.json artifact is still written and schema-valid regardless of format.
    let run = read_run_json(dir.path());
    assert_schema(&run);
    assert_eq!(run["exit_code"], 0);
}

#[test]
fn contract_run_default_text_keeps_stdout_clean() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: test\nmodel: dummy\ntests:\n  - id: pass\n    input: hello",
    )
    .unwrap();

    let out = Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .success()
        .get_output()
        .clone();

    // Default text format leaves stdout empty (human report goes to stderr),
    // so `--format json` is the only thing that writes machine output to stdout.
    assert!(
        out.stdout.is_empty(),
        "default text run must not write to stdout, got: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// ---------------------------------------------------------------------------
// Vacuous / unparsable `expected:` blocks
//
// A YAML typo in `expected:` used to fall back to `Expected::default()` — an
// empty `must_contain`, which passes for any response. These pin the three
// silent paths to the default as config errors (exit 2), end to end.
// ---------------------------------------------------------------------------

#[test]
fn contract_run_rejects_unparsable_expected_object() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: typo
model: dummy
tests:
  - id: t1
    input: hello
    expected:
      must_contains: ["hello"]
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_run_rejects_unrecognized_expected_list_entry() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: typo
model: dummy
tests:
  - id: t1
    input: hello
    expected:
      - must_contains: ["hello"]
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_run_rejects_tagged_fallback_to_different_legacy_metric() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: mismatched-tag
model: dummy
tests:
  - id: t1
    input: hello
    expected:
      type: regex_match
      must_contain: "not-the-dummy-output"
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_run_rejects_ambiguous_legacy_expected_block() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: ambiguous-legacy
model: dummy
tests:
  - id: t1
    input: hello
    expected:
      must_contain: "passed"
      sequence: ["Search"]
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

#[test]
fn contract_run_rejects_multi_element_expected_list() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: multi
model: dummy
tests:
  - id: t1
    input: hello
    expected:
      - must_contain: "hello"
      - must_contain: "world"
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

/// An explicitly-written empty assertion is a HARD ERROR, and it must be caught on
/// the paths that decide outcomes — `run` and `ci`, not just `validate`.
#[test]
fn contract_run_rejects_explicitly_empty_assertion() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: vacuous
model: dummy
tests:
  - id: always_green
    input: hello
    expected:
      type: must_contain
      must_contain: []
"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .arg("run")
        .arg("--config")
        .arg("assay.yaml")
        .assert()
        .code(2);

    let run = read_run_json(dir.path());
    assert_eq!(run["reason_code"], "E_CFG_PARSE");
}

/// A test that omits `expected:` AND has no `assertions:` asserts nothing, but the
/// omission itself is a documented, legitimate shape — so `assay validate` sweeps for
/// it as a WARNING (exit 0), not an error.
#[test]
fn contract_validate_warns_on_test_with_no_assertion() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        r#"suite: vacuous
model: dummy
tests:
  - id: always_green
    input: hello
"#,
    )
    .unwrap();

    let out = Command::cargo_bin("assay")
        .unwrap()
        .current_dir(dir.path())
        .arg("validate")
        .arg("--config")
        .arg("assay.yaml")
        .arg("--format")
        .arg("json")
        .assert()
        .code(0)
        .get_output()
        .clone();

    let report: Value = serde_json::from_slice(&out.stdout).expect("validate json");
    let diags = report["diagnostics"].as_array().expect("diagnostics array");
    let vacuous: Vec<&Value> = diags
        .iter()
        .filter(|d| d["code"] == "W_CFG_VACUOUS_EXPECTED")
        .collect();
    assert_eq!(
        vacuous.len(),
        1,
        "expected one vacuous warning, got {:?}",
        diags
    );
    assert_eq!(vacuous[0]["severity"], "warn");
}

// ---------------------------------------------------------------------------
// One failure, one report.
//
// `into_exit_code` reports every classified failure, so the sites that also printed for
// themselves produced two reports for one condition -- in two different wordings, which is
// the arrangement that drifts. The `try_map_error` site was worse than duplicated: its
// message was a rendered diagnostic, so the reported diagnostic framed an already framed
// one and the result was unreadable.
// ---------------------------------------------------------------------------

fn stderr_of(dir: &std::path::Path, args: &[&str]) -> String {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.current_dir(dir).env("ASSAY_EXIT_CODES", "v2");
    for arg in args {
        cmd.arg(arg);
    }
    let out = cmd.assert().code(2).get_output().clone();
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn contract_arg_conflict_is_reported_once() {
    let dir = tempdir().unwrap();
    let stderr = stderr_of(
        dir.path(),
        &["run", "--baseline", "dummy", "--export-baseline", "dummy"],
    );

    assert_eq!(
        stderr.matches("-export-baseline").count(),
        1,
        "the conflict was reported more than once:\n{}",
        stderr
    );
    assert!(stderr.contains("E_INVALID_ARGS"), "{}", stderr);
}

#[test]
fn contract_missing_trace_file_is_reported_once() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: t\nmodel: trace\ntests:\n  - id: t1\n    input: hello\n",
    )
    .unwrap();

    let stderr = stderr_of(dir.path(), &["run", "--config", "assay.yaml"]);

    assert_eq!(
        stderr.matches("--trace-file <PATH>").count(),
        1,
        "the requirement was reported more than once:\n{}",
        stderr
    );
}

#[test]
fn contract_upstream_diagnostic_is_reported_once_and_not_nested() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("assay.yaml"),
        "suite: demo\nmodel: dummy\ntests:\n  - id: t1\n    input: hello\n",
    )
    .unwrap();
    // A schema version the loader refuses, which `try_map_error` classifies as a baseline
    // mismatch before the pipeline turns it into a `PipelineError`.
    fs::write(
        dir.path().join("baseline.json"),
        r#"{"schema_version":99,"suite":"demo","assay_version":"0.0.0","created_at":"2026-01-01T00:00:00Z","config_fingerprint":"abc","entries":[]}"#,
    )
    .unwrap();

    let stderr = stderr_of(
        dir.path(),
        &[
            "run",
            "--config",
            "assay.yaml",
            "--baseline",
            "baseline.json",
        ],
    );

    assert_eq!(
        stderr
            .matches("Baseline incompatible with current run")
            .count(),
        1,
        "the diagnostic was rendered inside itself:\n{}",
        stderr
    );
    // What the classifier knew survives the fold: its own code, its context, its fix steps.
    assert!(stderr.contains("E_BASE_MISMATCH"), "{}", stderr);
    assert!(
        stderr.contains("unsupported baseline schema version 99"),
        "{}",
        stderr
    );
    assert!(
        stderr.contains("Regenerate baseline on main branch"),
        "{}",
        stderr
    );

    // The artifact carries a message, not a block of terminal output.
    let v = read_run_json(dir.path());
    let message = v["resolution"]["message"].as_str().expect("message");
    assert!(
        !message.contains('\n'),
        "run.json message must not be a rendering: {:?}",
        message
    );
}
