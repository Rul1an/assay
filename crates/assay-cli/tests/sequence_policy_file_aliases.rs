//! Contract: a `sequence_valid` policy file that declares `aliases` never lets a test pass on rules
//! evaluated without them.
//!
//! When a test runs, its `policy:` file is read once and bound into the test's rules, so that
//! validation, fingerprinting and the metric all see the same snapshot. That snapshot has room for
//! the rules and not for the aliases, so a rule naming an alias was evaluated against the literal
//! alias name. A `before` whose `then` is an alias then never saw its consequent and reported
//! `not_exercised`, and the test passed on a trace the rule forbids.
//!
//! The binding now refuses such a file with a configuration error. A policy file without aliases
//! is unaffected.

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

const EVAL: &str = "configVersion: 1\nsuite: sequence_policy_aliases\nmodel: trace\nsettings:\n  cache: false\n\
tests:\n  - id: policy_file_rules\n    input:\n      prompt: \"seq\"\n    expected:\n      type: sequence_valid\n      policy: policy.yaml\n";

fn trace_jsonl(tools: &[&str]) -> String {
    let mut lines = vec![json!({
        "type": "episode_start", "episode_id": "e1", "timestamp": 1000,
        "input": {"prompt": "seq"}
    })];
    for (i, tool) in tools.iter().enumerate() {
        lines.push(json!({
            "type": "tool_call", "episode_id": "e1", "step_id": format!("s{i}"),
            "timestamp": 1001 + i as u64, "tool_name": tool, "call_index": i, "args": {}
        }));
    }
    lines.push(json!({
        "type": "episode_end", "episode_id": "e1", "timestamp": 2000, "final_output": "done"
    }));
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn run(dir: &Path, sub: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off")
        .args([
            sub,
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            ":memory:",
        ])
        .output()
        .expect("run assay binary")
}

fn suite(policy: &str, tools: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("eval.yaml"), EVAL).expect("eval.yaml");
    std::fs::write(dir.path().join("policy.yaml"), policy).expect("policy.yaml");
    std::fs::write(dir.path().join("trace.jsonl"), trace_jsonl(tools)).expect("trace.jsonl");
    dir
}

fn describe(out: &Output) -> String {
    format!(
        "exit={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn rule_outcome(dir: &Path) -> Value {
    let run_json: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("run.json")).expect("run.json written"),
    )
    .expect("run.json is JSON");
    run_json["results"][0]["details"]["metrics"]["sequence_valid"]["details"]["rule_evaluations"][0]
        ["outcome"]
        .clone()
}

/// `Search` is an alias for `A` and `B`. The trace calls `A` with no `Login` before it, which the
/// rule forbids. It must not pass, through either command that runs a suite.
#[test]
fn an_aliased_policy_file_never_passes_a_trace_its_rule_forbids() {
    let policy = "version: \"1.0\"\naliases:\n  Search: [A, B]\nsequences:\n  - { type: before, first: Login, then: Search }\n";
    for sub in ["run", "ci"] {
        let dir = suite(policy, &["A"]);
        let out = run(dir.path(), sub);
        assert_eq!(
            out.status.code(),
            Some(2),
            "assay {sub} must refuse the aliased policy file\n{}",
            describe(&out)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("aliases") && stderr.contains("policy.yaml"),
            "assay {sub} must name the file and the aliases\n{stderr}"
        );
    }
}

/// The same shape without aliases still runs, and decides both ways.
#[test]
fn a_policy_file_without_aliases_still_runs() {
    let policy = "version: \"1.0\"\nsequences:\n  - { type: before, first: Login, then: Search }\n";
    let dir = suite(policy, &["Search"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert_eq!(rule_outcome(dir.path()), "violated");

    let dir = suite(policy, &["Login", "Search"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(rule_outcome(dir.path()), "held");
}

/// A policy file whose `aliases` key is present but empty declares no alias and runs.
#[test]
fn an_empty_aliases_map_is_not_a_declared_alias() {
    let policy = "version: \"1.0\"\naliases: {}\nsequences:\n  - { type: require, tool: A }\n";
    let dir = suite(policy, &["A"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(rule_outcome(dir.path()), "held");
}
