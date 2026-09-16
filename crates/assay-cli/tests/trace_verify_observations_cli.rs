//! `assay trace verify` reaches the observation reader (#2782).
//!
//! The in-process tests in `assay-core` prove the reader rules; this file proves the
//! binary actually routes through them: rows on stdout, `--trust-stage` repeated and
//! empty by default, and the existing coverage exit semantics untouched.

use std::path::Path;
use std::process::{Command, Output};

const CONFIG: &str = r#"configVersion: 1
suite: trace-verify-observations
model: fake
tests:
  - id: hello
    input:
      prompt: "hello"
    expected:
      type: must_contain
      must_contain: ["hello"]
"#;

fn write_fixture(dir: &Path, meta_len: usize) -> (std::path::PathBuf, std::path::PathBuf) {
    let trace = dir.join("trace.jsonl");
    let line = serde_json::json!({
        "type": "episode_start",
        "episode_id": "ep-1",
        "timestamp": 1,
        "input": {"prompt": "hello"},
        "meta": {"note": "m".repeat(meta_len)}
    });
    std::fs::write(&trace, format!("{line}\n")).unwrap();
    let config = dir.join("eval.yaml");
    std::fs::write(&config, CONFIG).unwrap();
    (trace, config)
}

fn verify(trace: &Path, config: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["trace", "verify", "--trace"])
        .arg(trace)
        .arg("--config")
        .arg(config)
        .args(extra)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

#[test]
fn default_run_prints_unmeasured_rows_and_keeps_exit_zero_on_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let (trace, config) = write_fixture(dir.path(), 1);
    let out = verify(&trace, &config, &[]);
    let text = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains(
            "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=unmeasured"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/meta reading=unmeasured"
        ),
        "{text}"
    );
    assert!(!text.contains("measured_clean"), "{text}");
    assert!(text.contains("Trace Verification Passed"), "{text}");
}

#[test]
fn trust_stage_is_repeatable_and_only_the_named_stage_counts() {
    let dir = tempfile::tempdir().unwrap();
    let (trace, config) = write_fixture(dir.path(), 1);

    let unrelated = stdout(&verify(
        &trace,
        &config,
        &["--trust-stage", "vendor.exporter"],
    ));
    assert!(!unrelated.contains("measured_clean"), "{unrelated}");
    assert!(unrelated.contains("reading=unmeasured"), "{unrelated}");

    let out = verify(
        &trace,
        &config,
        &[
            "--trust-stage",
            "vendor.exporter",
            "--trust-stage",
            "assay.trace.upgrader",
        ],
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        text.contains(
            "pointer=/input reading=measured_clean stage=\"assay.trace.upgrader\" ceiling=4096"
        ),
        "{text}"
    );
}

#[test]
fn reported_loss_survives_an_empty_trust_list() {
    let dir = tempfile::tempdir().unwrap();
    let (trace, config) = write_fixture(dir.path(), 5000);
    let out = verify(&trace, &config, &[]);
    let text = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "metadata loss does not change coverage"
    );
    assert!(text.contains("pointer=/meta reading=lossy"), "{text}");
}

#[test]
fn rows_are_printed_when_coverage_fails_and_the_exit_code_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (trace, config) = write_fixture(dir.path(), 1);
    std::fs::write(
        &config,
        CONFIG.replace("prompt: \"hello\"", "prompt: \"absent\""),
    )
    .unwrap();
    let out = verify(&trace, &config, &[]);
    let text = stdout(&out);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("reading=unmeasured"), "{text}");
    assert!(!text.contains("Trace Verification Passed"), "{text}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("missing matching prompt in trace"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
