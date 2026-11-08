//! Contract: `assay validate --trace-file` decides an `args_valid` test the way `assay run`
//! decides it on the same trace.
//!
//! `docs/reference/cli/validate.md` says validate checks the trace's tool calls against the
//! schema rules in the config, and `docs/reference/policies.md` documents the policy those rules
//! come from. Validate read a different policy than `assay run` does: it refused the documented
//! `schemas:` form and the top-level `allow`/`deny` lists, ignored an inline `schema:` and a
//! `tools.allow`/`tools.deny` list, and read only `tools.arg_constraints`, a field `assay run`
//! refuses. A per-argument map under that field validated as OK on a call that breaks it.
//!
//! Every case below runs both commands. `assay run` is the reference: validate must reach the
//! same exit, and report a failed test, by its id, exactly when run fails it.

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
        "configVersion: 1\nsuite: validate_args_rules\nmodel: trace\nsettings:\n  cache: false\n\
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

fn validate_json(dir: &Path) -> Output {
    assay(
        dir,
        &[
            "validate",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--format",
            "json",
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

/// The error diagnostics in a JSON validate report that name test `id`.
fn errors_for(report: &Value, id: &str) -> Vec<Value> {
    report["diagnostics"]
        .as_array()
        .map(|diags| {
            diags
                .iter()
                .filter(|d| d["severity"] == "error" && d["context"]["test_id"] == id)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
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

const POLICY_REF: &str = "      type: args_valid\n      policy: policy.yaml";

const TMP_ONLY: &str =
    "{type: object, required: [path], properties: {path: {type: string, pattern: \"^/tmp/\"}}}";

struct Case {
    name: &'static str,
    expected: String,
    policy: Option<String>,
    calls: Vec<(&'static str, Value)>,
    run_exit: i32,
}

/// One row per verdict shape: every policy form `assay run` reads (an inline schema map, a
/// `version: "2.0"` policy with `schemas:`, top-level and `tools.` allow and deny lists,
/// `enforcement.unconstrained_tools: deny`, a legacy schema-map file), each broken and kept, plus
/// a trace with no calls and a violation in a later call.
fn cases() -> Vec<Case> {
    let read = |path: &str| ("read_file", json!({ "path": path }));
    let file = |s: String| Some(s);
    let by_ref = || POLICY_REF.to_string();
    vec![
        Case {
            name: "inline schema map, broken",
            expected: format!("      type: args_valid\n      schema:\n        read_file: {TMP_ONLY}"),
            policy: None,
            calls: vec![read("/etc/passwd")],
            run_exit: 1,
        },
        Case {
            name: "inline schema map, kept",
            expected: format!("      type: args_valid\n      schema:\n        read_file: {TMP_ONLY}"),
            policy: None,
            calls: vec![read("/tmp/x")],
            run_exit: 0,
        },
        Case {
            name: "version 2.0 schemas, broken",
            expected: by_ref(),
            policy: file(format!("version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\n")),
            calls: vec![read("/etc/passwd")],
            run_exit: 1,
        },
        Case {
            name: "version 2.0 schemas, kept",
            expected: by_ref(),
            policy: file(format!("version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\n")),
            calls: vec![read("/tmp/x")],
            run_exit: 0,
        },
        Case {
            name: "deny list, denied tool called",
            expected: by_ref(),
            policy: file("version: \"2.0\"\ndeny: [exec]\n".into()),
            calls: vec![("exec", json!({"cmd": "ls"}))],
            run_exit: 1,
        },
        Case {
            name: "deny list, other tool called",
            expected: by_ref(),
            policy: file("version: \"2.0\"\ndeny: [exec]\n".into()),
            calls: vec![read("/etc/passwd")],
            run_exit: 0,
        },
        Case {
            name: "tools.deny list, denied tool called",
            expected: by_ref(),
            policy: file("version: \"1.0\"\ntools:\n  deny: [exec]\n".into()),
            calls: vec![("exec", json!({"cmd": "ls"}))],
            run_exit: 1,
        },
        Case {
            name: "tools.deny list, other tool called",
            expected: by_ref(),
            policy: file("version: \"1.0\"\ntools:\n  deny: [exec]\n".into()),
            calls: vec![read("/etc/passwd")],
            run_exit: 0,
        },
        Case {
            name: "allow list, other tool called",
            expected: by_ref(),
            policy: file("version: \"2.0\"\nallow: [read_file]\n".into()),
            calls: vec![("write_file", json!({"path": "/tmp/x"}))],
            run_exit: 1,
        },
        Case {
            name: "unconstrained tools denied, tool without schema called",
            expected: by_ref(),
            policy: file(format!(
                "version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\nenforcement:\n  unconstrained_tools: deny\n"
            )),
            calls: vec![("write_file", json!({"path": "/tmp/x"}))],
            run_exit: 1,
        },
        Case {
            name: "legacy schema map file, broken",
            expected: by_ref(),
            policy: file(format!("read_file: {TMP_ONLY}\n")),
            calls: vec![read("/etc/passwd")],
            run_exit: 1,
        },
        Case {
            name: "schemas, a later call broken",
            expected: by_ref(),
            policy: file(format!("version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\n")),
            calls: vec![read("/tmp/x"), read("/etc/passwd")],
            run_exit: 1,
        },
        Case {
            name: "schemas, no calls in the trace",
            expected: by_ref(),
            policy: file(format!("version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\n")),
            calls: vec![],
            run_exit: 0,
        },
    ]
}

#[test]
fn validate_reaches_the_verdict_run_reaches_on_the_same_trace() {
    let mut mismatches = Vec::new();
    for case in cases() {
        let dir = suite(
            &eval_yaml(&case.expected),
            case.policy.as_deref(),
            &case.calls,
        );
        let ran = run(dir.path());
        assert_eq!(
            ran.status.code(),
            Some(case.run_exit),
            "precondition: `{}` is run exit {}\n{}",
            case.name,
            case.run_exit,
            describe(&ran)
        );
        let checked = validate_json(dir.path());
        if checked.status.code() != Some(case.run_exit) {
            mismatches.push(format!(
                "`{}`: run exit {}, validate {}",
                case.name,
                case.run_exit,
                describe(&checked)
            ));
            continue;
        }
        let report: Value = match serde_json::from_slice(&checked.stdout) {
            Ok(report) => report,
            Err(err) => {
                mismatches.push(format!(
                    "`{}`: no JSON report ({err})\n{}",
                    case.name,
                    describe(&checked)
                ));
                continue;
            }
        };
        let failed = errors_for(&report, "t");
        if case.run_exit == 1 && failed.is_empty() {
            mismatches.push(format!(
                "`{}`: validate exited 1 without an error naming test `t`\n{report:#}",
                case.name
            ));
        }
        if case.run_exit == 0 && !failed.is_empty() {
            mismatches.push(format!(
                "`{}`: validate passed but reported an error for test `t`\n{report:#}",
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

/// In a suite of two, only the test whose policy the trace breaks is reported, and by its id.
#[test]
fn validate_names_the_test_that_breaks_its_policy() {
    let eval = concat!(
        "configVersion: 1\nsuite: validate_args_rules\nmodel: trace\nsettings:\n  cache: false\n",
        "tests:\n",
        "  - id: holds\n    input:\n      prompt: \"p\"\n    expected:\n      type: args_valid\n",
        "      schema:\n        read_file: {type: object, properties: {path: {type: string}}}\n",
        "  - id: breaks\n    input:\n      prompt: \"p\"\n    expected:\n      type: args_valid\n",
        "      schema:\n        read_file: {type: object, properties: {path: {type: string, pattern: \"^/tmp/\"}}}\n",
    );
    let dir = suite(eval, None, &[("read_file", json!({"path": "/etc/passwd"}))]);
    assert_eq!(run(dir.path()).status.code(), Some(1));
    let out = validate_json(dir.path());
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    let report: Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    assert_eq!(errors_for(&report, "breaks").len(), 1, "{report:#}");
    assert!(errors_for(&report, "holds").is_empty(), "{report:#}");
}

/// Each `args_valid` input `assay run` refuses before a test runs, with the words validate's
/// report must carry. `None` as the policy is a missing file. Validate refuses each one as a
/// configuration error, with or without a trace, because the policy needs neither.
#[test]
fn validate_refuses_every_args_policy_that_run_refuses() {
    let by_ref = POLICY_REF.to_string();
    let refused: Vec<(&str, String, Option<String>, &str)> = vec![
        ("missing file", by_ref.clone(), None, "policy.yaml"),
        (
            "not YAML",
            by_ref.clone(),
            Some("schemas: [unclosed\n".into()),
            "policy.yaml",
        ),
        (
            "a tool schema under tools.arg_constraints",
            by_ref.clone(),
            Some(format!("version: \"1.0\"\ntools:\n  arg_constraints:\n    read_file: {TMP_ONLY}\n")),
            "tools.arg_constraints. Put each tool's JSON Schema under `schemas.<tool>`",
        ),
        (
            "a per-argument map under tools.arg_constraints",
            by_ref.clone(),
            Some("version: \"1.0\"\ntools:\n  arg_constraints:\n    read_file:\n      path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "tools.arg_constraints. Put each tool's JSON Schema under `schemas.<tool>`",
        ),
        (
            "a policy with no constraint",
            by_ref.clone(),
            Some("version: \"2.0\"\n".into()),
            "asserts nothing",
        ),
        (
            "a field run does not enforce",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [exec]\nconstraints: {}\n".into()),
            "not enforced",
        ),
    ];
    for (case, expected, policy, words) in &refused {
        let dir = suite(
            &eval_yaml(expected),
            policy.as_deref(),
            &[("read_file", json!({"path": "/tmp/x"}))],
        );
        let ran = run(dir.path());
        assert_eq!(
            ran.status.code(),
            Some(2),
            "precondition: assay run refuses {case}\n{}",
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
                "assay {} must refuse {case}\n{}",
                args.join(" "),
                describe(&out)
            );
            let report = text(&out);
            assert!(
                report.contains("E_CFG_PARSE") && report.contains(words),
                "assay {} must report {case} as E_CFG_PARSE naming `{words}`\n{report}",
                args.join(" ")
            );
        }
    }
}

/// Validate reads the policy file `assay run` reads. Both resolve `policy:` against the directory
/// they run in, so from the config's parent directory both fail to read it, for that reason.
#[test]
fn validate_reads_the_args_policy_file_run_reads() {
    let dir = suite(
        &eval_yaml(POLICY_REF),
        Some(&format!(
            "version: \"2.0\"\nschemas:\n  read_file: {TMP_ONLY}\n"
        )),
        &[("read_file", json!({"path": "/etc/passwd"}))],
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
    let unread = "failed to read args_valid policy 'policy.yaml'";
    assert!(
        ran.status.code() == Some(2) && text(&ran).contains(unread),
        "precondition: assay run does not find the policy from the parent directory\n{}",
        describe(&ran)
    );
    assert!(
        checked.status.code() == Some(2) && text(&checked).contains(unread),
        "validate must fail to read the policy run fails to read\nrun {}\nvalidate {}",
        describe(&ran),
        describe(&checked)
    );
}
