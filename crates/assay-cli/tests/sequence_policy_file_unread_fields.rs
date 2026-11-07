//! Contract: a `sequence_valid` policy file never declares a field the test silently ignores.
//!
//! A policy-form file is the same `Policy` document that `assay explain --policy` reads. A test
//! reads its `sequences` and nothing else, so `tools.allow`, `tools.deny`, `tools.require_args`,
//! `tools.arg_constraints` and `on_error` were accepted and had no effect: a file with
//! `tools: {deny: [rm]}` passed a trace that calls `rm`, while `assay explain` on the same file
//! blocked that call.
//!
//! Every reader that turns the file into a test's rules now refuses such a file with a
//! configuration error that names each field: `assay run`, `assay ci` and `assay migrate`.
//! `version`, `name` and `metadata` describe the file and stay accepted.

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

const EVAL: &str = "configVersion: 1\nsuite: sequence_policy_unread_fields\nmodel: trace\nsettings:\n  cache: false\n\
tests:\n  - id: policy_file_rules\n    input:\n      prompt: \"seq\"\n    expected:\n      type: sequence_valid\n      policy: policy.yaml\n";

fn trace_jsonl(tools: &[&str]) -> String {
    let mut lines = vec![json!({
        "type": "episode_start", "episode_id": "e1", "timestamp": 1000,
        "input": {"prompt": "seq"}
    })];
    for (i, tool) in tools.iter().enumerate() {
        lines.push(json!({
            "type": "tool_call", "episode_id": "e1", "step_id": format!("s{i}"),
            "timestamp": 1001 + i as u64, "tool_name": tool, "call_index": i,
            "args": {"path": "/etc/passwd"}
        }));
    }
    lines.push(json!({
        "type": "episode_end", "episode_id": "e1", "timestamp": 2000, "final_output": "done"
    }));
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn assay(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off")
        .args(args)
        .output()
        .expect("run assay binary")
}

const RUN: &[&str] = &[
    "--config",
    "eval.yaml",
    "--trace-file",
    "trace.jsonl",
    "--db",
    ":memory:",
];

fn run(dir: &Path, sub: &str) -> Output {
    let mut args = vec![sub];
    args.extend_from_slice(RUN);
    assay(dir, &args)
}

fn migrate(dir: &Path) -> Output {
    assay(dir, &["migrate", "--config", "eval.yaml"])
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
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
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

/// The refusal names the file and lists the fields, sorted, in one message.
fn assert_refused(out: &Output, cmd: &str, fields: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let text = format!("{stdout}{stderr}");
    assert!(
        text.contains("policy.yaml") && text.contains(&format!("does not enforce ({fields})")),
        "assay {cmd} must name the file and the fields ({fields})\n{}",
        describe(out)
    );
}

/// One field per row: the policy file, the trace, and the field the refusal must name. Each
/// trace is one the field's plain reading forbids, and the rule alone allows, so a reader that
/// ignores the field passes it.
const UNREAD: &[(&str, &[&str], &str)] = &[
    (
        "version: \"1.0\"\ntools:\n  deny: [rm]\nsequences:\n  - { type: require, tool: rm }\n",
        &["rm"],
        "tools.deny",
    ),
    (
        "version: \"1.0\"\ntools:\n  allow: [ls]\nsequences:\n  - { type: require, tool: rm }\n",
        &["rm"],
        "tools.allow",
    ),
    (
        "version: \"1.0\"\ntools:\n  require_args:\n    read_file: [mode]\nsequences:\n  - { type: require, tool: read_file }\n",
        &["read_file"],
        "tools.require_args",
    ),
    (
        "version: \"1.0\"\ntools:\n  arg_constraints:\n    read_file:\n      path: { type: string, pattern: \"^/tmp/\" }\nsequences:\n  - { type: require, tool: read_file }\n",
        &["read_file"],
        "tools.arg_constraints",
    ),
    (
        "version: \"1.0\"\non_error: allow\nsequences:\n  - { type: require, tool: A }\n",
        &["A"],
        "on_error",
    ),
];

/// The probe that found it: a deny list the test never read, on a trace that calls the denied
/// tool. Neither command that runs a suite may pass it.
#[test]
fn a_deny_list_in_a_sequence_policy_file_never_passes_a_call_it_denies() {
    let (policy, tools, field) = UNREAD[0];
    for sub in ["run", "ci"] {
        let dir = suite(policy, tools);
        let out = run(dir.path(), sub);
        assert_eq!(
            out.status.code(),
            Some(2),
            "assay {sub} must refuse the file\n{}",
            describe(&out)
        );
        assert_refused(&out, sub, field);
    }
}

/// Every field a test does not read is refused by `assay run`, each named.
#[test]
fn every_unread_policy_field_is_refused_by_name() {
    for (policy, tools, field) in UNREAD {
        let dir = suite(policy, tools);
        let out = run(dir.path(), "run");
        assert_eq!(
            out.status.code(),
            Some(2),
            "assay run must refuse a file declaring {field}\n{}",
            describe(&out)
        );
        assert_refused(&out, "run", field);
    }
}

/// Several unread fields are named together, sorted, so one edit fixes the file.
#[test]
fn several_unread_fields_are_named_in_one_refusal() {
    let policy = "version: \"1.0\"\ntools:\n  deny: [rm]\n  allow: [ls]\non_error: block\nsequences:\n  - { type: require, tool: rm }\n";
    let dir = suite(policy, &["rm"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert_refused(&out, "run", "on_error, tools.allow, tools.deny");
}

/// `assay migrate` writes the rules inline and would drop the field the same way. It refuses
/// and leaves the config as it was.
#[test]
fn assay_migrate_refuses_an_unread_field_and_writes_nothing() {
    let (policy, tools, field) = UNREAD[0];
    let dir = suite(policy, tools);
    let out = migrate(dir.path());
    assert_ne!(out.status.code(), Some(0), "{}", describe(&out));
    assert_refused(&out, "migrate", field);
    let config = std::fs::read_to_string(dir.path().join("eval.yaml")).expect("eval.yaml");
    assert_eq!(config, EVAL, "assay migrate must not rewrite the config");
}

/// A file that declares only what a test reads, plus the fields that describe it, still runs and
/// decides both ways. An empty `tools` mapping declares nothing.
#[test]
fn a_policy_file_with_only_read_and_descriptive_fields_still_runs() {
    let policy = "version: \"1.0\"\nname: login_first\nmetadata:\n  owner: team\ntools: {}\nsequences:\n  - { type: before, first: Login, then: Search }\n";
    let dir = suite(policy, &["Search"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert_eq!(rule_outcome(dir.path()), "violated");

    let dir = suite(policy, &["Login", "Search"]);
    let out = run(dir.path(), "run");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(rule_outcome(dir.path()), "held");

    let out = migrate(dir.path());
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
}
