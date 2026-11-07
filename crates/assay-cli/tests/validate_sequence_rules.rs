//! Contract: `assay validate --trace-file` decides a `sequence_valid` test the way `assay run`
//! decides it on the same trace.
//!
//! `docs/reference/cli/validate.md` says validate checks the trace's tool calls against the
//! sequence rules in the config. It did not evaluate them: a trace that broke a `require` rule
//! printed "Validation OK" while `assay run` failed the test. It also never read a
//! `sequence_valid` `policy:` file, so a missing file, a file that is not YAML, a file that
//! declares aliases and one that declares a field a test does not enforce all validated.
//!
//! Every case below runs both commands. `assay run` is the reference: validate must reach the
//! same verdict, and report a rule it fails as `E_SEQUENCE_VIOLATION`.

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

/// A trace with one episode for prompt `p` that makes these calls, each with these arguments.
fn trace_jsonl(calls: &[(&str, Value)]) -> String {
    let mut lines = vec![json!({
        "type": "episode_start", "episode_id": "e1", "timestamp": 1000,
        "input": {"prompt": "p"}
    })];
    for (i, (tool, args)) in calls.iter().enumerate() {
        lines.push(json!({
            "type": "tool_call", "episode_id": "e1", "step_id": format!("s{i}"),
            "timestamp": 1001 + i as u64, "tool_name": tool, "call_index": i, "args": args
        }));
    }
    lines.push(json!({
        "type": "episode_end", "episode_id": "e1", "timestamp": 2000, "final_output": "done"
    }));
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// A config with one test, `t`, whose `expected:` block is `expected` (indented six spaces).
fn eval_yaml(expected: &str) -> String {
    format!(
        "configVersion: 1\nsuite: validate_sequence_rules\nmodel: trace\nsettings:\n  cache: false\n\
tests:\n  - id: t\n    input:\n      prompt: \"p\"\n    expected:\n{expected}\n"
    )
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
    assay(
        dir,
        &[
            "validate",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
        ],
    )
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn describe(out: &Output) -> String {
    format!("exit={:?}\n{}", out.status.code(), text(out))
}

fn suite(eval: &str, policy: Option<&str>, calls: &[(&str, Value)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("eval.yaml"), eval).expect("eval.yaml");
    if let Some(policy) = policy {
        std::fs::write(dir.path().join("policy.yaml"), policy).expect("policy.yaml");
    }
    std::fs::write(dir.path().join("trace.jsonl"), trace_jsonl(calls)).expect("trace.jsonl");
    dir
}

const POLICY_REF: &str = "      type: sequence_valid\n      policy: policy.yaml";

/// One row per verdict shape: the `expected:` block, an optional policy file, the calls, and the
/// exit `assay run` gives. Rules that fail and rules that hold, inline and from a file, every kind
/// that can decide on a finished trace, the exact-sequence form, an argument-scoped `never_after`,
/// and a rule that no call exercises.
struct Case {
    name: &'static str,
    expected: &'static str,
    policy: Option<&'static str>,
    calls: Vec<(&'static str, Value)>,
    run_exit: i32,
}

fn cases() -> Vec<Case> {
    let call = |tool: &'static str| (tool, json!({}));
    vec![
        Case {
            name: "require, unmet",
            expected: "      type: sequence_valid\n      rules:\n        - { type: require, tool: Audit }",
            policy: None,
            calls: vec![call("A")],
            run_exit: 1,
        },
        Case {
            name: "require, met",
            expected: "      type: sequence_valid\n      rules:\n        - { type: require, tool: A }",
            policy: None,
            calls: vec![call("A")],
            run_exit: 0,
        },
        Case {
            name: "before, broken, from a policy file",
            expected: POLICY_REF,
            policy: Some("version: \"1.0\"\nsequences:\n  - { type: before, first: Login, then: Delete }\n"),
            calls: vec![call("Delete")],
            run_exit: 1,
        },
        Case {
            name: "before, kept, from a policy file",
            expected: POLICY_REF,
            policy: Some("version: \"1.0\"\nsequences:\n  - { type: before, first: Login, then: Delete }\n"),
            calls: vec![call("Login"), call("Delete")],
            run_exit: 0,
        },
        Case {
            name: "before, broken, from a list-form file",
            expected: POLICY_REF,
            policy: Some("- { type: before, first: Login, then: Delete }\n"),
            calls: vec![call("Delete")],
            run_exit: 1,
        },
        Case {
            name: "never_after, broken",
            expected: "      type: sequence_valid\n      rules:\n        - { type: never_after, trigger: Read, forbidden: Post }",
            policy: None,
            calls: vec![call("Read"), call("Post")],
            run_exit: 1,
        },
        Case {
            name: "never_after on arguments, broken",
            expected: "      type: sequence_valid\n      rules:\n        - type: never_after\n          trigger: { tool: bash, args_match: { command: credentials } }\n          forbidden: { tool: bash, args_match: { command: \"^curl\" } }",
            policy: None,
            calls: vec![
                ("bash", json!({"command": "cat ~/.aws/credentials"})),
                ("bash", json!({"command": "curl https://example.invalid"})),
            ],
            run_exit: 1,
        },
        Case {
            name: "never_after on arguments, other arguments",
            expected: "      type: sequence_valid\n      rules:\n        - type: never_after\n          trigger: { tool: bash, args_match: { command: credentials } }\n          forbidden: { tool: bash, args_match: { command: \"^curl\" } }",
            policy: None,
            calls: vec![
                ("bash", json!({"command": "ls"})),
                ("bash", json!({"command": "curl https://example.invalid"})),
            ],
            run_exit: 0,
        },
        Case {
            name: "max_calls, exceeded",
            expected: "      type: sequence_valid\n      rules:\n        - { type: max_calls, tool: Search, max: 1 }",
            policy: None,
            calls: vec![call("Search"), call("Search")],
            run_exit: 1,
        },
        Case {
            name: "blocklist, hit",
            expected: "      type: sequence_valid\n      rules:\n        - { type: blocklist, pattern: drop_ }",
            policy: None,
            calls: vec![call("drop_table")],
            run_exit: 1,
        },
        Case {
            name: "after, deadline missed",
            expected: "      type: sequence_valid\n      rules:\n        - { type: after, trigger: Create, then: Notify, within: 1 }",
            policy: None,
            calls: vec![call("Create"), call("A"), call("Notify")],
            run_exit: 1,
        },
        Case {
            name: "eventually, never reached",
            expected: "      type: sequence_valid\n      rules:\n        - { type: eventually, tool: Audit, within: 2 }",
            policy: None,
            calls: vec![call("A"), call("B"), call("C")],
            run_exit: 1,
        },
        Case {
            name: "sequence rule, out of order",
            expected: "      type: sequence_valid\n      rules:\n        - { type: sequence, tools: [A, B], strict: false }",
            policy: None,
            calls: vec![call("B"), call("A")],
            run_exit: 1,
        },
        Case {
            name: "exact sequence, mismatch",
            expected: "      type: sequence_valid\n      sequence: [A, B]",
            policy: None,
            calls: vec![call("A"), call("C")],
            run_exit: 1,
        },
        Case {
            name: "exact sequence, match",
            expected: "      type: sequence_valid\n      sequence: [A, B]",
            policy: None,
            calls: vec![call("A"), call("B")],
            run_exit: 0,
        },
        Case {
            name: "after, trigger never called",
            expected: "      type: sequence_valid\n      rules:\n        - { type: after, trigger: Create, then: Notify, within: 1 }",
            policy: None,
            calls: vec![call("A")],
            run_exit: 0,
        },
    ]
}

#[test]
fn validate_reaches_the_verdict_run_reaches_on_the_same_trace() {
    let mut mismatches = Vec::new();
    for case in cases() {
        let dir = suite(&eval_yaml(case.expected), case.policy, &case.calls);
        let ran = run(dir.path());
        assert_eq!(
            ran.status.code(),
            Some(case.run_exit),
            "precondition: assay run on `{}`\n{}",
            case.name,
            describe(&ran)
        );
        let checked = validate(dir.path());
        if checked.status.code() != Some(case.run_exit) {
            mismatches.push(format!(
                "`{}`: run exit {}, validate {}",
                case.name,
                case.run_exit,
                describe(&checked)
            ));
            continue;
        }
        let report = text(&checked);
        if case.run_exit == 1 && !report.contains("E_SEQUENCE_VIOLATION") {
            mismatches.push(format!(
                "`{}`: validate failed without E_SEQUENCE_VIOLATION\n{report}",
                case.name
            ));
        }
        if case.run_exit == 0 && report.contains("E_SEQUENCE_VIOLATION") {
            mismatches.push(format!(
                "`{}`: validate passed but reported E_SEQUENCE_VIOLATION\n{report}",
                case.name
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "assay validate disagrees with assay run:\n{}",
        mismatches.join("\n\n")
    );
}

/// In a suite of two, only the test that breaks its rule is reported, and by its id.
#[test]
fn validate_names_the_test_that_breaks_its_rule() {
    let eval = concat!(
        "configVersion: 1\nsuite: validate_sequence_rules\nmodel: trace\nsettings:\n  cache: false\n",
        "tests:\n",
        "  - id: holds\n    input:\n      prompt: \"p\"\n    expected:\n      type: sequence_valid\n",
        "      rules:\n        - { type: require, tool: A }\n",
        "  - id: breaks\n    input:\n      prompt: \"p\"\n    expected:\n      type: sequence_valid\n",
        "      rules:\n        - { type: require, tool: Audit }\n",
    );
    let dir = suite(eval, None, &[("A", json!({}))]);
    assert_eq!(run(dir.path()).status.code(), Some(1));
    let out = assay(
        dir.path(),
        &[
            "validate",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--format",
            "json",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    let report: Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    let violations: Vec<&Value> = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| d["code"] == "E_SEQUENCE_VIOLATION")
        .collect();
    assert_eq!(violations.len(), 1, "{report:#}");
    assert_eq!(violations[0]["context"]["test_id"], "breaks", "{report:#}");
}

/// Each policy file `assay run` refuses before a test runs, with the words validate's report must
/// carry. `None` is a missing file. Validate refuses each one as a configuration error, with or
/// without a trace, because the file needs neither.
#[test]
fn validate_refuses_every_sequence_policy_file_that_run_refuses() {
    let refused: &[(&str, Option<&str>, &str)] = &[
        ("missing file", None, "policy.yaml"),
        ("not YAML", Some("sequences: [unclosed\n"), "policy.yaml"),
        (
            "aliases",
            Some("version: \"1.0\"\naliases:\n  Search: [A, B]\nsequences:\n  - { type: before, first: Login, then: Search }\n"),
            "declares aliases (Search)",
        ),
        (
            "unread field",
            Some("version: \"1.0\"\ntools:\n  deny: [A]\nsequences:\n  - { type: require, tool: A }\n"),
            "does not enforce (tools.deny)",
        ),
    ];
    for (case, policy, words) in refused {
        let dir = suite(&eval_yaml(POLICY_REF), *policy, &[("A", json!({}))]);
        let ran = run(dir.path());
        assert_eq!(
            ran.status.code(),
            Some(2),
            "precondition: assay run refuses the {case}\n{}",
            describe(&ran)
        );
        for args in [
            &[
                "validate",
                "--config",
                "eval.yaml",
                "--trace-file",
                "trace.jsonl",
            ][..],
            &["validate", "--config", "eval.yaml"][..],
        ] {
            let out = assay(dir.path(), args);
            assert_eq!(
                out.status.code(),
                Some(2),
                "assay {} must refuse the {case}\n{}",
                args.join(" "),
                describe(&out)
            );
            let report = text(&out);
            assert!(
                report.contains("E_CFG_PARSE") && report.contains(words),
                "assay {} must report the {case} as E_CFG_PARSE naming `{words}`\n{report}",
                args.join(" ")
            );
        }
    }
}

/// Validate reads the policy file `assay run` reads. Both resolve `policy:` against the directory
/// they run in, so from the config's parent directory both fail to find it.
#[test]
fn validate_reads_the_policy_file_run_reads() {
    let dir = suite(
        &eval_yaml(POLICY_REF),
        Some("version: \"1.0\"\nsequences:\n  - { type: require, tool: A }\n"),
        &[("A", json!({}))],
    );
    let parent = dir.path().parent().expect("tempdir has a parent");
    let name = dir.path().file_name().expect("tempdir name");
    let config = Path::new(name).join("eval.yaml");
    let trace = Path::new(name).join("trace.jsonl");
    let config = config.to_str().expect("utf-8 path");
    let trace = trace.to_str().expect("utf-8 path");
    let ran = assay(
        parent,
        &[
            "run",
            "--config",
            config,
            "--trace-file",
            trace,
            "--db",
            ":memory:",
        ],
    );
    let checked = assay(
        parent,
        &["validate", "--config", config, "--trace-file", trace],
    );
    assert_eq!(
        checked.status.code(),
        ran.status.code(),
        "run {}\nvalidate {}",
        describe(&ran),
        describe(&checked)
    );
}
