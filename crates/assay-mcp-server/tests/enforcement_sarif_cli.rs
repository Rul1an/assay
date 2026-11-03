#[path = "../../../tests/support/bounded_process.rs"]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

fn run_projection<S: AsRef<OsStr>>(input: &[u8], args: &[S]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
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
        .env("NO_COLOR", "1")
        .arg("enforcement-sarif")
        .arg("--input")
        .arg("-")
        .args(args);
    run_bounded(
        command,
        input,
        GOLDEN_PATH_LIMITS,
        "enforcement-sarif projection",
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

fn deny_record() -> String {
    json!({
        "schema": "assay.enforcement_decision.v0",
        "tool": {"name": "github.add_deploy_key", "action_class": "github_deploy_key"},
        "decision": "deny",
        "reason": "no_declared_allowance",
        "fail_closed": true,
        "drift_state": "not_evaluated"
    })
    .to_string()
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_invalid_line(output: &Output, line: usize) {
    assert_eq!(
        output.status.code(),
        Some(1),
        "invalid NDJSON must exit 1; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "rejected input must not produce a successful SARIF document"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("input line {line}")), "{stderr}");
    assert!(stderr.contains("expected one JSON value"), "{stderr}");
    assert!(
        !stderr.contains("not-json"),
        "the rejected record contents must not be echoed: {stderr}"
    );
}

#[test]
fn malformed_first_line_fails_without_sarif() {
    let output = run_projection(b"not-json\n", &["--output", "-"]);
    assert_invalid_line(&output, 1);
}

#[test]
fn malformed_later_line_does_not_materialize_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output_path = dir.path().join("report.sarif");
    let input = format!("{}\nnot-json\n", deny_record());
    let output = run_projection(
        input.as_bytes(),
        &[OsStr::new("--output"), output_path.as_os_str()],
    );
    assert_invalid_line(&output, 2);
    assert!(
        !Path::new(&output_path).exists(),
        "rejected input must not materialize a SARIF output file"
    );
}

#[test]
fn blank_lines_and_valid_denies_are_accepted() {
    let input = format!("\n  \n{}\n\n", deny_record());
    let output = run_projection(input.as_bytes(), &["--output", "-"]);
    assert!(
        output.status.success(),
        "blank lines are allowed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout_json(&output)["runs"][0]["results"]
            .as_array()
            .expect("SARIF results")
            .len(),
        1
    );
}

#[test]
fn valid_non_enforcement_records_remain_ignored() {
    let input = format!(
        "{}\n{}\n",
        json!({"schema": "assay.manifest_establish.v0", "status": "complete"}),
        deny_record()
    );
    let output = run_projection(input.as_bytes(), &["--output", "-"]);
    assert!(
        output.status.success(),
        "non-enforcement JSON remains outside this projection; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = stdout_json(&output)["runs"][0]["results"]
        .as_array()
        .expect("SARIF results")
        .clone();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ruleId"], "no_declared_allowance");
}

#[test]
fn oversized_input_fails_before_projection() {
    const LIMIT: usize = 16 * 1024 * 1024;
    let output = run_projection(&vec![b'x'; LIMIT + 1], &["--output", "-"]);

    assert!(!output.status.success(), "oversized input must fail");
    assert!(output.stdout.is_empty(), "oversized input emitted SARIF");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("input exceeds 16777216-byte limit"),
        "missing bounded-input diagnosis: {stderr}"
    );
}

#[test]
fn oversized_input_reports_limit_when_boundary_splits_utf8() {
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut input = vec![b'x'; LIMIT];
    input.extend_from_slice("€".as_bytes());
    let output = run_projection(&input, &["--output", "-"]);

    assert!(!output.status.success(), "oversized input must fail");
    assert!(output.stdout.is_empty(), "oversized input emitted SARIF");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("input exceeds 16777216-byte limit"),
        "size classification was hidden by UTF-8 decoding: {stderr}"
    );
}

fn file_projection(input: &Path, output: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    command
        .arg("enforcement-sarif")
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output);
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "file projection").unwrap()
}

fn refuses_input_alias(kind: &str) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("decisions.ndjson");
    let bytes = format!("{}\n", deny_record()).into_bytes();
    std::fs::write(&input, &bytes).unwrap();
    let alias = dir.path().join("output.sarif");
    let output = match kind {
        "same" => input.clone(),
        "hardlink" => {
            std::fs::hard_link(&input, &alias).unwrap();
            alias
        }
        #[cfg(unix)]
        "symlink" => {
            std::os::unix::fs::symlink(&input, &alias).unwrap();
            alias
        }
        _ => panic!("unknown alias"),
    };
    let result = file_projection(&input, &output);
    assert_eq!(
        std::fs::read(&input).unwrap(),
        bytes,
        "source changed: {kind}"
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("input"));
}
#[test]
fn refuses_same_input_path() {
    refuses_input_alias("same");
}
#[test]
fn refuses_hardlinked_input() {
    refuses_input_alias("hardlink");
}
#[cfg(unix)]
#[test]
fn refuses_symlinked_input() {
    refuses_input_alias("symlink");
}

#[test]
fn overwrites_distinct_output_without_changing_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("decisions.ndjson");
    let output = dir.path().join("report.sarif");
    let bytes = format!("{}\n", deny_record()).into_bytes();
    std::fs::write(&input, &bytes).unwrap();
    std::fs::write(&output, vec![b'x'; 65536]).unwrap();
    let result = file_projection(&input, &output);
    assert!(result.status.success(), "{:?}", result);
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read(&input).unwrap(), bytes);
    let actual: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(
        actual,
        stdout_json(&run_projection(&bytes, &["--output", "-"]))
    );
}
