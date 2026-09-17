//! Prompt-refusal contract for `assay fix` and `assay doctor --fix` (#2573 slice 1).
//!
//! When stdin is not a terminal the process cannot show a confirm prompt. The old
//! copies turned that into a silent "no" (`interact().unwrap_or(false)`), so
//! `assay fix` printed `No patches applied.` and exited 0 — a false clean. A
//! refused prompt is a usage condition and must exit 2, naming `--yes`.
//!
//! Known remaining prompt (out of scope for this slice):
//! `crates/assay-cli/src/cli/commands/runner_builder.rs` — `assay run` with an
//! OpenAI embedder and no `OPENAI_API_KEY` prints `Enter key:` and blocks on
//! `stdin.read_line`. That is a secret prompt, not a confirm.

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

/// Copied from `doctor_fix_e2e.rs` so this contract reuses the same tree T1 names.
fn write_minimal_config(path: &Path) {
    fs::write(
        path,
        "version: 1\nsuite: doctor-fix\nmodel: trace\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write eval config");
}

/// Parse-error repair `doctor --fix` can offer: a misspelled `settings` key
/// (same fixture bytes as `doctor_fix_e2e.rs`). Used when `assay fix --list`
/// yields no patch: `validate()` emits `E_ARG_SCHEMA` for a failing
/// `args_valid` call, and `build_suggestions` has no patch arm for that code.
fn write_parse_error_config(path: &Path) {
    fs::write(
        path,
        "version: 1\nsuite: doctor-fix\nmodel: trace\nsettngs: {}\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write misspelled eval config");
}

/// Smallest `assay fix` tree that produces a tool-call diagnostic.
///
/// Fixture: `eval.yaml` (`args_valid` → `policy.yaml`) + a matching trace whose
/// `read_file` args fail the schema (`path` is a number). `validate` reports
/// `E_ARG_SCHEMA`. That is the smallest validate input that carries a tool call;
/// it does not currently become a `SuggestedPatch` (see module comment).
fn write_fix_patch_tree(dir: &Path) -> PathBuf {
    let config = dir.join("eval.yaml");
    fs::write(
        &config,
        r#"version: 1
suite: prompt-refusal
model: trace
tests:
  - id: t1
    input:
      prompt: "hello"
    expected:
      type: args_valid
      policy: policy.yaml
"#,
    )
    .expect("write eval.yaml");
    fs::write(
        dir.join("policy.yaml"),
        r#"version: "1.0"
name: prompt-refusal
tools:
  arg_constraints:
    read_file:
      type: object
      additionalProperties: false
      properties:
        path:
          type: string
      required: [path]
"#,
    )
    .expect("write policy.yaml");
    fs::create_dir_all(dir.join("traces")).expect("traces/");
    fs::write(
        dir.join("traces/main.jsonl"),
        r#"{"schema_version": 1, "type": "assay.trace", "request_id": "hello_1", "prompt": "hello", "response": "ok", "model": "trace", "provider": "trace", "meta": {"tool_calls": [{"tool_name": "read_file", "args": {"path": 1}}]}}
"#,
    )
    .expect("write trace");
    config
}

fn run_null_stdin(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut std_cmd = StdCommand::new(env!("CARGO_BIN_EXE_assay"));
    std_cmd
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .args(args);
    Command::from_std(std_cmd).assert()
}

fn listed_patches(dir: &Path, config: &Path) -> String {
    let assert = run_null_stdin(
        dir,
        &[
            "fix",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            "traces/main.jsonl",
            "--list",
        ],
    )
    .success();
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

fn assert_refused(assert: assert_cmd::assert::Assert, context: &str) {
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().expect("exit code");
    assert_eq!(
        code, 2,
        "{context}: refused prompt must exit 2; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--yes"),
        "{context}: refused prompt must name --yes on stderr; stderr:\n{stderr}"
    );
}

#[test]
fn t1_doctor_fix_without_yes_refuses_when_stdin_is_not_a_terminal() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");
    write_minimal_config(&config);
    assert!(!trace.exists());

    let assert = run_null_stdin(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            trace.to_str().expect("utf8 trace"),
            "--fix",
        ],
    );
    assert_refused(assert, "doctor --fix");
    assert!(
        !trace.exists(),
        "a refused prompt must not create traces/main.jsonl"
    );
}

#[test]
fn t2_fix_without_yes_refuses_when_stdin_is_not_a_terminal() {
    let temp = tempdir().expect("tempdir");
    let config = write_fix_patch_tree(temp.path());
    let listed = listed_patches(temp.path(), &config);
    let before = fs::read(&config).expect("read config before");

    if listed.trim().is_empty() {
        // Item 2 (#2573 slice 2): no measured `validate()` input produces a
        // `SuggestedPatch`. `fix.rs` runs `validate` then `build_suggestions`.
        // Patch arms need codes `validate` never emits (`E_CFG_SCHEMA_UNKNOWN_FIELD`
        // with file/pointer context, `E_TOOL_NOT_ALLOWED`, `E_PATH_SCOPE_VIOLATION`)
        // or `E_PATH_NOT_FOUND` with `file` ending `assay.yaml` plus `field`,
        // which `validate` does not attach (it sets `path` only). The confirm
        // is unreachable today; do not invent a patch arm here. The parse-error
        // doctor path below is the reachable apply-prompt.
        let assert = run_null_stdin(
            temp.path(),
            &[
                "fix",
                "--config",
                config.to_str().expect("utf8 config"),
                "--trace-file",
                "traces/main.jsonl",
            ],
        );
        let after = fs::read(&config).expect("read config after");
        assert_eq!(before, after, "assay fix must not change the config");
        let output = assert.get_output();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("No patches applied."),
            "empty-patch path is 'No suggested patches', not the declined-apply false clean; \
             stderr:\n{stderr}"
        );
        return;
    }

    let assert = run_null_stdin(
        temp.path(),
        &[
            "fix",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            "traces/main.jsonl",
        ],
    );
    assert_refused(assert, "assay fix");
    let after = fs::read(&config).expect("read config after");
    assert_eq!(before, after, "a refused prompt must not change the config");
}

#[test]
fn t2_doctor_parse_error_without_yes_refuses_when_stdin_is_not_a_terminal() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    write_parse_error_config(&config);
    let before = fs::read(&config).expect("read config before");

    let assert = run_null_stdin(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--fix",
        ],
    );
    assert_refused(assert, "doctor --fix parse-error");
    let after = fs::read(&config).expect("read config after");
    assert_eq!(
        before, after,
        "a refused parse-error repair must not change the config"
    );
}

#[test]
fn t3_fix_with_yes_applies_the_patch_when_one_exists() {
    let temp = tempdir().expect("tempdir");
    let config = write_fix_patch_tree(temp.path());
    let listed = listed_patches(temp.path(), &config);
    if listed.trim().is_empty() {
        return;
    }
    let policy = temp.path().join("policy.yaml");
    let before_policy = fs::read(&policy).expect("read policy before");

    let assert = run_null_stdin(
        temp.path(),
        &[
            "fix",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            "traces/main.jsonl",
            "--yes",
        ],
    );
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code = output.status.code().expect("exit code");
    assert_eq!(
        code, 0,
        "assay fix --yes should apply and exit 0; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let after_policy = fs::read(&policy).expect("read policy after");
    assert_ne!(
        before_policy, after_policy,
        "assay fix --yes must apply the suggested patch; stderr:\n{stderr}"
    );
}

#[test]
fn t3_doctor_parse_error_with_yes_applies_the_rename() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    write_parse_error_config(&config);
    let before = fs::read_to_string(&config).expect("read config before");
    let misspelled_line = before.lines().nth(3).expect("fixture has misspelled key");

    let assert = run_null_stdin(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--fix",
            "--yes",
        ],
    )
    .code(0);
    let after = fs::read_to_string(&config).expect("read config after");
    assert!(
        after.contains("settings:"),
        "doctor --fix --yes must write the settings key; after:\n{after}"
    );
    assert!(
        !after.lines().any(|line| line == misspelled_line),
        "doctor --fix --yes must drop the misspelled key; after:\n{after}"
    );
    let _ = assert;
}

#[test]
fn t3_doctor_fix_dry_run_without_yes_does_not_prompt() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");
    write_minimal_config(&config);

    let assert = run_null_stdin(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            trace.to_str().expect("utf8 trace"),
            "--fix",
            "--dry-run",
        ],
    )
    .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        !trace.exists(),
        "doctor --fix --dry-run must not create the trace"
    );
    assert!(
        !stderr.contains("--yes") && !stdout.contains("--yes"),
        "--dry-run is consent; it must not refuse as an unshowable prompt; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn t3_fix_dry_run_without_yes_is_consent() {
    let temp = tempdir().expect("tempdir");
    let config = write_fix_patch_tree(temp.path());
    let listed = listed_patches(temp.path(), &config);
    let before = fs::read(&config).expect("read config");
    let before_policy = fs::read(temp.path().join("policy.yaml")).expect("read policy");

    let assert = run_null_stdin(
        temp.path(),
        &[
            "fix",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            "traces/main.jsonl",
            "--dry-run",
        ],
    );
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code = output.status.code().expect("exit code");
    if listed.trim().is_empty() {
        assert_eq!(
            code, 0,
            "empty-patch dry-run stays the no-work exit; stdout:\n{stdout}\nstderr:\n{stderr}"
        );
    } else {
        assert_eq!(
            code, 0,
            "assay fix --dry-run is preapproved consent and must not refuse; \
             stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("--yes"),
            "--dry-run must not name --yes as a missing flag; stderr:\n{stderr}"
        );
    }
    assert_eq!(before, fs::read(&config).expect("config after"));
    assert_eq!(
        before_policy,
        fs::read(temp.path().join("policy.yaml")).expect("policy after"),
        "dry-run must not write the patch"
    );
}

/// M2: `fix.rs` must pass `yes || dry_run` into `confirm`. A binary case cannot
/// bite that until `validate` emits a patch-producing code; this is the case
/// the brief says to add when none exists.
#[test]
fn fix_rs_consent_predicate_is_yes_or_dry_run() {
    let src = include_str!("../src/cli/commands/fix.rs");
    assert!(
        src.contains("args.yes || args.dry_run"),
        "fix.rs must use the unified consent predicate yes || dry_run at confirm"
    );
}
