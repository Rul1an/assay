//! An eval config key that nothing acts on is refused when the config loads, by name, in every
//! command that loads a config.
//!
//! These keys are part of the config schema, so neither `deny_unknown_fields` nor the unknown-key
//! warning sees them, and a config that sets them used to pass `assay validate` and run as if they
//! were not there: `bail_on_first_failure: true` ran every test, `thresholds.min_score` set no
//! threshold, `settings.thresholding.min_floor` set no floor. The refusal is in the one config read
//! every command goes through, so the four commands below cannot disagree.

use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;

const TRACE: &str = r#"{"schema_version":1,"type":"assay.trace","request_id":"r1","prompt":"p1","response":"hello","model":"trace","provider":"trace"}
"#;

const PASSING_TEST: &str =
    "tests:\n  - id: says_hello\n    input: { prompt: \"p1\" }\n    expected: { type: must_contain, must_contain: [\"hello\"] }\n";

/// A judge reliability block with every field, as the schema requires one, and `tie_break` given.
fn reliability(tie_break: &str) -> String {
    format!(
        "  judge:\n    reliability:\n      borderline_min: 0.4\n      borderline_max: 0.6\n      rerun_strategy: sequential_sprt\n      max_extra_calls_per_test: 2\n      max_extra_calls_per_run: 20\n      tie_break: {tie_break}\n      blind_labeling: true\n      order_randomized: true\n      hijack_defense: true\n"
    )
}

fn write_config(dir: &Path, body: &str) -> std::path::PathBuf {
    std::fs::write(dir.join("trace.jsonl"), TRACE).expect("trace");
    let path = dir.join("eval.yaml");
    std::fs::write(
        &path,
        format!("version: 1\nsuite: keys_probe\nmodel: trace\n{body}"),
    )
    .expect("config");
    path
}

/// Exit code and all output of one command on the config in `dir`.
fn assay(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run assay");
    (
        output.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

const COMMANDS: [&[&str]; 4] = [
    &[
        "validate",
        "--config",
        "eval.yaml",
        "--trace-file",
        "trace.jsonl",
    ],
    &[
        "run",
        "--config",
        "eval.yaml",
        "--trace-file",
        "trace.jsonl",
        "--db",
        ":memory:",
    ],
    &[
        "ci",
        "--config",
        "eval.yaml",
        "--trace-file",
        "trace.jsonl",
        "--db",
        ":memory:",
    ],
    &["doctor", "--config", "eval.yaml"],
];

#[test]
fn a_key_nothing_acts_on_is_refused_by_every_command_that_loads_the_config() {
    let semantic_test_with_max_drop = "tests:\n  - id: similar\n    input: { prompt: \"p1\" }\n    expected:\n      type: semantic_similarity_to\n      semantic_similarity_to: \"hello\"\n      thresholding: { max_drop: 0.1 }\n";
    let cases: Vec<(String, &str)> = vec![
        (format!("thresholds:\n  min_score: 0.9\n{PASSING_TEST}"), "thresholds.min_score"),
        (format!("thresholds:\n  block_on_warn: true\n{PASSING_TEST}"), "thresholds.block_on_warn"),
        (
            format!("settings:\n  bail_on_first_failure: true\n{PASSING_TEST}"),
            "settings.bail_on_first_failure",
        ),
        (
            format!("settings:\n  thresholding:\n    mode: relative\n    min_floor: 0.8\n{PASSING_TEST}"),
            "settings.thresholding.min_floor",
        ),
        (
            format!("settings:\n  thresholding:\n    mode: baseline\n{PASSING_TEST}"),
            "settings.thresholding.mode",
        ),
        (
            format!("settings:\n  thresholding:\n    max_drop: 0.05\n{PASSING_TEST}"),
            "settings.thresholding.max_drop",
        ),
        (
            semantic_test_with_max_drop.to_string(),
            "tests.similar.expected.thresholding.max_drop",
        ),
        (
            format!("settings:\n  judge:\n    samples: 3\n{PASSING_TEST}"),
            "settings.judge.samples",
        ),
        (
            format!("settings:\n  judge:\n    rubric_version: v1\n{PASSING_TEST}"),
            "settings.judge.rubric_version",
        ),
        (
            format!("settings:\n{}{PASSING_TEST}", reliability("quarantine")),
            "settings.judge.reliability.tie_break",
        ),
    ];
    // Every case and command is tried, so a failure lists all that are not refused.
    let mut not_refused = Vec::new();
    for (body, key) in &cases {
        let dir = TempDir::new().expect("tempdir");
        write_config(dir.path(), body);
        for command in COMMANDS {
            let (code, output) = assay(dir.path(), command);
            if code != Some(2) || !output.contains(&format!("{key} has no effect")) {
                not_refused.push(format!(
                    "{key} under `assay {}` (exit {code:?})",
                    command[0]
                ));
            }
        }
        let (_, output) = assay(dir.path(), COMMANDS[0]);
        if !output.contains("E_CFG_PARSE") {
            not_refused.push(format!("{key}: validate does not name E_CFG_PARSE"));
        }
    }
    assert!(
        not_refused.is_empty(),
        "not refused:\n{}",
        not_refused.join("\n")
    );
}

/// Control: the same keys with values the run does act on, or that state what it does, load and
/// run. Without this, refusing every config would pass the test above.
#[test]
fn the_keys_a_run_acts_on_load_and_run() {
    let dir = TempDir::new().expect("tempdir");
    write_config(
        dir.path(),
        &format!(
            "settings:\n  cache: false\n  timeout_seconds: 10\n  bail_on_first_failure: false\n  thresholding:\n    mode: relative\n    max_drop: 0.05\n{}{PASSING_TEST}",
            reliability("fail_closed")
        ),
    );
    for command in COMMANDS {
        let (code, output) = assay(dir.path(), command);
        assert_eq!(code, Some(0), "`assay {}`: {output}", command[0]);
        assert!(
            !output.contains("has no effect"),
            "`assay {}`: {output}",
            command[0]
        );
    }

    let absolute = TempDir::new().expect("tempdir");
    write_config(
        absolute.path(),
        &format!("settings:\n  thresholding:\n    mode: absolute\n{PASSING_TEST}"),
    );
    let (code, output) = assay(absolute.path(), COMMANDS[0]);
    assert_eq!(code, Some(0), "{output}");
}

/// The config reference lists each key refused above, so the table a user reads is the list the
/// loader refuses.
#[test]
fn the_config_reference_lists_every_refused_key() {
    let reference = include_str!("../../../docs/reference/config/eval-yaml.md");
    let table = reference
        .split("### Keys that are refused because nothing acts on them")
        .nth(1)
        .and_then(|rest| rest.split("\n---").next())
        .expect("the refused-keys section");
    for key in [
        "thresholds.min_score",
        "thresholds.block_on_warn",
        "settings.bail_on_first_failure",
        "settings.thresholding.mode",
        "settings.thresholding.max_drop",
        "expected.thresholding.max_drop",
        "settings.thresholding.min_floor",
        "settings.judge.samples",
        "settings.judge.rubric_version",
        "settings.judge.reliability.tie_break",
    ] {
        assert!(
            table.contains(&format!("`{key}")),
            "{key} is not in the reference table"
        );
    }
}
