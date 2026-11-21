//! A key a sequence rule does not take refuses the config, by name, wherever the rule is written:
//! inline under `rules:`, or in the file a `sequence_valid` test names, as a list of rules or as a
//! policy's `sequences`.
//!
//! The rule types used to drop such a key. `strict` misspelt on a `sequence` rule left the rule
//! non-strict, so a trace with a call between its two tools passed (exit 0) a check written to fail
//! it, and `assay validate` reported nothing.

use serde_json::json;
use std::path::Path;
use std::process::{Command, Output};

/// The trace every case runs: `a`, then `c`, then `b`. A strict `[a, b]` sequence fails it.
fn trace_jsonl() -> String {
    let mut lines = vec![json!({
        "type": "episode_start", "episode_id": "e1", "timestamp": 1000,
        "input": {"prompt": "seq"}
    })];
    for (i, tool) in ["a", "c", "b"].iter().enumerate() {
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

const HEAD: &str = "configVersion: 1\nsuite: sequence_rule_unknown_keys\nmodel: trace\nsettings:\n  \
                    cache: false\ntests:\n  - id: seq\n    input:\n      prompt: \"seq\"\n    expected:\n      \
                    type: sequence_valid\n";

/// The three places a rule is written, each as (label, eval config, policy file), with `{key}` where
/// the rule's `strict` key goes.
const PLACES: &[(&str, &str, Option<&str>)] = &[
    (
        "inline rules",
        "      rules:\n        - type: sequence\n          tools: [a, b]\n          {key}: true\n",
        None,
    ),
    (
        "a policy file of rules",
        "      policy: policy.yaml\n",
        Some("- type: sequence\n  tools: [a, b]\n  {key}: true\n"),
    ),
    (
        "a policy file's sequences",
        "      policy: policy.yaml\n",
        Some("version: \"1.0\"\nsequences:\n  - type: sequence\n    tools: [a, b]\n    {key}: true\n"),
    ),
];

fn suite(expected: &str, policy: Option<&str>, key: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("eval.yaml"),
        format!("{HEAD}{}", expected.replace("{key}", key)),
    )
    .expect("eval.yaml");
    if let Some(policy) = policy {
        std::fs::write(dir.path().join("policy.yaml"), policy.replace("{key}", key))
            .expect("policy.yaml");
    }
    std::fs::write(dir.path().join("trace.jsonl"), trace_jsonl()).expect("trace.jsonl");
    dir
}

fn assay(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run assay binary")
}

fn run(dir: &Path) -> Output {
    assay(
        dir,
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            ":memory:",
        ],
    )
}

fn validate(dir: &Path) -> Output {
    assay(dir, &["validate", "--config", "eval.yaml"])
}

fn describe(out: &Output) -> String {
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The control: spelt right, the rule is strict in every place, and the trace fails it.
#[test]
fn a_strict_sequence_rule_fails_a_trace_with_a_call_between_its_tools() {
    for (place, expected, policy) in PLACES {
        let dir = suite(expected, *policy, "strict");
        if policy.is_some() {
            let out = assay(dir.path(), &["migrate", "--config", "eval.yaml"]);
            assert!(out.status.success(), "{place}: {}", describe(&out));
        }
        let out = run(dir.path());
        assert_eq!(
            out.status.code(),
            Some(1),
            "{place}: a strict rule must fail the trace\n{}",
            describe(&out)
        );
    }
}

/// Misspelt, `assay validate` and `assay run` refuse the config with a configuration error that
/// names the key, before any trace is judged.
#[test]
fn a_key_a_sequence_rule_does_not_take_refuses_the_config_by_name() {
    for (place, expected, policy) in PLACES {
        let dir = suite(expected, *policy, "strictly");
        let reason = if policy.is_some() {
            "assay migrate"
        } else {
            "unknown field `strictly`"
        };
        for (command, out) in [("validate", validate(dir.path())), ("run", run(dir.path()))] {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                out.status.code() == Some(2) && text.contains(reason),
                "{place}: assay {command} must refuse the config and name the key\n{}",
                describe(&out)
            );
        }
    }
}

/// `assay migrate` inlines the rules it reads, so it refuses the same key, and names it for a rule
/// in a policy file as it does for an inline rule.
#[test]
fn assay_migrate_names_a_key_a_sequence_rule_does_not_take() {
    for (place, expected, policy) in PLACES {
        let dir = suite(expected, *policy, "strictly");
        let reason = match policy {
            None => "unknown field `strictly`",
            Some(doc) if doc.starts_with('-') => "0.strictly",
            Some(_) => "sequences.0.strictly",
        };
        let out = assay(dir.path(), &["migrate", "--config", "eval.yaml"]);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.code() == Some(2) && text.contains(reason),
            "{place}: assay migrate must refuse the config and name the key\n{}",
            describe(&out)
        );
    }
}

/// `assay coverage --policy` reads the policy's rules to report which of them a trace exercises.
/// A rule with a key it does not take refuses the policy by name, as a policy that cannot be read
/// does, instead of reporting coverage of no rules. Spelt right, the same file loads.
#[test]
fn assay_coverage_refuses_a_policy_whose_rule_has_a_key_it_does_not_take() {
    const POLICY: &str =
        "version: \"2.0\"\nsequences:\n  - type: sequence\n    tools: [a, b]\n    {key}: true\n";
    for (key, refused) in [("strict", false), ("strictly", true)] {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("policy.yaml"), POLICY.replace("{key}", key))
            .expect("policy.yaml");
        std::fs::write(dir.path().join("trace.jsonl"), trace_jsonl()).expect("trace.jsonl");
        let out = assay(
            dir.path(),
            &[
                "coverage",
                "--policy",
                "policy.yaml",
                "--trace-file",
                "trace.jsonl",
            ],
        );
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if refused {
            assert!(
                out.status.code() == Some(2) && text.contains("sequences.0.strictly"),
                "assay coverage must refuse the policy and name the key\n{}",
                describe(&out)
            );
        } else {
            assert_eq!(
                out.status.code(),
                Some(0),
                "spelt right, the policy loads\n{}",
                describe(&out)
            );
        }
    }
}
