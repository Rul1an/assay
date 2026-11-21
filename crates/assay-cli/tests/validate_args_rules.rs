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
            expected: format!("      type: args_valid\n      schema:\n        version: '2.0'\n        schemas:\n          read_file: {TMP_ONLY}"),
            policy: None,
            calls: vec![read("/etc/passwd")],
            run_exit: 1,
        },
        Case {
            name: "inline schema map, kept",
            expected: format!("      type: args_valid\n      schema:\n        version: '2.0'\n        schemas:\n          read_file: {TMP_ONLY}"),
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
        if case.policy.is_some() {
            let migrated = assay(dir.path(), &["migrate", "--config", "eval.yaml"]);
            assert!(
                migrated.status.success(),
                "{}: {}",
                case.name,
                describe(&migrated)
            );
        }
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
        "      schema:\n        version: '2.0'\n        schemas:\n          read_file: {type: object, properties: {path: {type: string}}}\n",
        "  - id: breaks\n    input:\n      prompt: \"p\"\n    expected:\n      type: args_valid\n",
        "      schema:\n        version: '2.0'\n        schemas:\n          read_file: {type: object, properties: {path: {type: string, pattern: \"^/tmp/\"}}}\n",
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
        (
            "misspelled inline enforcement without a separate deny list",
            "      type: args_valid\n      schema: {version: '2', enforcement: {unconstrained: deny}}".into(),
            None,
            "enforcement.unconstrained",
        ),
        (
            "invalid unconstrained tool mode without a separate deny list",
            "      type: args_valid\n      schema: {version: '2', enforcement: {unconstrained_tools: drop}}".into(),
            None,
            "enforcement.unconstrained_tools must be one of",
        ),
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
        // Each tool's schema is checked on its own, whatever else the policy asserts. A deny list,
        // `enforcement.unconstrained_tools: deny` or another tool's schema used to make the
        // policy effective, and then a tool schema that constrains nothing passed the call it was
        // written for.
        // A structured policy refuses every root field the evaluator does not read, as it refused
        // `constraints` and the other wrap-only fields. A misspelled or misplaced field used to be
        // ignored, so a deny list kept the policy effective while the schemas it misspelled
        // checked nothing.
        (
            "a misspelled schemas beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschema:\n  read_file:\n    type: object\n    properties:\n      path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "not enforced by this evaluator: schema",
        ),
        (
            "a sequence field beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nsequences:\n  - {type: require, tool: audit}\n".into()),
            "not enforced by this evaluator: sequences",
        ),
        (
            "an enforcement key the evaluator does not read",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nenforcement:\n  unconstrained: deny\n".into()),
            "not enforced by this evaluator: enforcement.unconstrained",
        ),
        (
            "shared definitions at the root instead of under schemas",
            by_ref.clone(),
            Some("version: \"2.0\"\n$defs:\n  p: {type: string, pattern: \"^/tmp/\"}\nschemas:\n  read_file:\n    type: object\n    properties:\n      path: {type: string}\n".into()),
            "not enforced by this evaluator: $defs",
        ),
        (
            "a per-argument map beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschemas:\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a per-argument map before another tool's schema",
            by_ref.clone(),
            Some("version: \"2.0\"\nschemas:\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n  exec:\n    type: object\n    required: [cmd]\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a per-argument map after another tool's schema",
            by_ref.clone(),
            Some("version: \"2.0\"\nschemas:\n  exec:\n    type: object\n    required: [cmd]\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "an inline per-argument map beside another tool's schema",
            "      type: args_valid\n      schema:\n        version: '2.0'\n        schemas:\n          read_file:\n            path: {type: string, pattern: \"^/tmp/\"}\n          exec:\n            type: object\n            required: [cmd]".to_string(),
            None,
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a per-argument map that also lists required arguments",
            by_ref.clone(),
            Some("version: \"2.0\"\nschemas:\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n    required: [path]\n  exec:\n    type: object\n    required: [cmd]\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a per-argument map beside tools.deny",
            by_ref.clone(),
            Some("version: \"2.0\"\ntools:\n  deny: [rm]\nschemas:\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a per-argument map under unconstrained_tools: deny",
            by_ref.clone(),
            Some("version: \"2.0\"\nenforcement:\n  unconstrained_tools: deny\nschemas:\n  read_file:\n    path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "an arguments map beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschemas:\n  read_file:\n    arguments:\n      path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: arguments",
        ),
        (
            "an argument beside type: object instead of under properties",
            by_ref.clone(),
            Some("version: \"2.0\"\nschemas:\n  read_file:\n    type: object\n    path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: path",
        ),
        (
            "a draft-3 keyword the evaluator does not read",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschemas:\n  read_file:\n    type: object\n    extends:\n      properties:\n        path: {type: string, pattern: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' has keys this evaluator does not read, so they constrain nothing: extends",
        ),
        (
            "nested constraints the evaluator does not read beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschemas:\n  read_file:\n    properties:\n      path: {kind: string, regex: \"^/tmp/\"}\n".into()),
            "schema for tool 'read_file' asserts nothing",
        ),
        (
            "a property with only a description beside a deny list",
            by_ref.clone(),
            Some("version: \"2.0\"\ndeny: [rm]\nschemas:\n  read_file:\n    properties:\n      path: {description: \"a path under /tmp\"}\n".into()),
            "schema for tool 'read_file' asserts nothing",
        ),
    ];
    for (case, expected, policy, words) in &refused {
        let dir = suite(
            &eval_yaml(expected),
            policy.as_deref(),
            &[("read_file", json!({"path": "/tmp/x"}))],
        );
        if expected == POLICY_REF && !words.starts_with("tools.arg_constraints.") {
            let migrated = assay(
                dir.path(),
                &["migrate", "--dry-run", "--config", "eval.yaml"],
            );
            assert!(
                !migrated.status.success() && text(&migrated).contains(words),
                "{case}: {}",
                describe(&migrated)
            );
        }
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
            let words = if expected == POLICY_REF {
                "assay migrate"
            } else {
                *words
            };
            assert!(
                report.contains("E_CFG_PARSE") && report.contains(words),
                "assay {} must report {case} as E_CFG_PARSE naming `{words}`\n{report}",
                args.join(" ")
            );
        }
    }
}

/// Both commands refuse the external check when invoked from the config's parent directory.
#[test]
fn run_and_validate_require_migration_from_the_parent_directory() {
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
    let unread = "assay migrate";
    assert!(
        ran.status.code() == Some(2) && text(&ran).contains(unread),
        "assay run must request migration\n{}",
        describe(&ran)
    );
    assert!(
        checked.status.code() == Some(2) && text(&checked).contains(unread),
        "validate must request migration\nrun {}\nvalidate {}",
        describe(&ran),
        describe(&checked)
    );
}

#[test]
fn duplicate_deny_key_cannot_replace_the_deny_list() {
    let eval = eval_yaml(POLICY_REF);
    for second_key in ["deny", "!key deny"] {
        let policy = format!("version: '2'\ndeny: [read_file]\n{second_key}: [other]");
        let dir = suite(&eval, Some(&policy), &[("read_file", json!({}))]);
        let output = assay(dir.path(), &["migrate", "--config", "eval.yaml"]);
        assert!(
            !output.status.success() && text(&output).contains("duplicate"),
            "{}",
            describe(&output)
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("eval.yaml")).unwrap(),
            eval
        );
    }
}

#[test]
fn args_policy_file_is_bounded_before_decoding() {
    let mut policy = String::from("version: '2'\ndeny: [other]\n#");
    policy.push_str(&"x".repeat(1_000_000 - policy.len()));
    let dir = suite(
        &eval_yaml(POLICY_REF),
        Some(&policy),
        &[("read_file", json!({}))],
    );
    let preview = || {
        assay(
            dir.path(),
            &["migrate", "--dry-run", "--config", "eval.yaml"],
        )
    };
    assert!(preview().status.success());
    policy.push('x');
    std::fs::write(dir.path().join("policy.yaml"), policy).unwrap();
    let output = preview();
    assert!(
        output.status.code() == Some(2) && text(&output).contains("source bytes limit of 1000000"),
        "{}",
        describe(&output)
    );
}

#[test]
fn config_only_trace_verification_refuses_invalid_inline_args_controls() {
    for (schema, reason) in [
        (
            "{version: '2', enforcement: {unconstrained_tools: drop}}",
            "enforcement.unconstrained_tools",
        ),
        (
            "{version: '2', enforcement: {unconstrained: deny}}",
            "enforcement.unconstrained",
        ),
        ("{version: 2, allow: ['*']}", "version must be a string"),
    ] {
        let expected = format!("      type: args_valid\n      schema: {schema}");
        let dir = suite(&eval_yaml(&expected), None, &[("read_file", json!({}))]);
        let output = assay(
            dir.path(),
            &[
                "trace",
                "verify",
                "--trace",
                "trace.jsonl",
                "--config",
                "eval.yaml",
            ],
        );
        assert!(
            output.status.code() == Some(2) && text(&output).contains(reason),
            "{}",
            describe(&output)
        );
    }
}

#[test]
fn migrate_wraps_a_tool_name_map_once_and_preserves_unlisted_calls() {
    let map = json!({"Search": {"type":"object", "required":["q"]}});
    let external = "      type: args_valid\n      policy: policy.yaml";
    let inline = format!("      type: args_valid\n      schema: {map}");
    for expected in [external, &inline] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let original = eval_yaml(expected);
        std::fs::write(dir.join("eval.yaml"), &original).unwrap();
        std::fs::write(dir.join("policy.yaml"), map.to_string()).unwrap();
        let migrate = |opts: &[&str]| {
            let mut args = vec!["migrate", "--config", "eval.yaml"];
            args.extend_from_slice(opts);
            assay(dir, &args)
        };
        let check_verdicts = || {
            for (tool, args, code) in [
                ("Search", json!({"q":"hi"}), 0),
                ("Search", json!({}), 1),
                ("Other", json!({}), 0),
            ] {
                std::fs::write(dir.join("trace.jsonl"), trace_jsonl(&[(tool, args)])).unwrap();
                for output in [run(dir), validate_json(dir)] {
                    assert_eq!(output.status.code(), Some(code), "{}", text(&output));
                }
            }
        };
        let refused = validate_json(dir);
        assert_eq!(refused.status.code(), Some(2));
        assert!(text(&refused).contains("assay migrate"));
        assert_eq!(migrate(&["--check"]).status.code(), Some(2));
        assert!(migrate(&[]).status.success());
        let raw = std::fs::read_to_string(dir.join("eval.yaml")).unwrap();
        let cfg: Value = serde_yaml::from_str(&raw).unwrap();
        let schema = &cfg["tests"][0]["expected"]["schema"];
        assert_eq!(schema["version"], "2.0");
        assert_eq!(schema["schemas"], map);
        assert_eq!(
            std::fs::read_to_string(dir.join("eval.yaml.bak")).unwrap(),
            original
        );
        std::fs::remove_file(dir.join("policy.yaml")).unwrap();
        assert!(migrate(&["--check"]).status.success());
        check_verdicts();
    }
}

#[test]
fn migration_does_not_admit_an_invalid_map_by_wrapping_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // A shared definitions entry is valid in a structured policy. The map reader refuses
    // JSON Schema keywords as tool names, so wrapping before validation changes the contract.
    let original = eval_yaml("      type: args_valid\n      policy: policy.yaml");
    std::fs::write(dir.join("eval.yaml"), &original).unwrap();
    let policy = r##"{"$defs":{"params":{"type":"object"}},"Search":{"$ref":"#/$defs/params"}}"##;
    std::fs::write(dir.join("policy.yaml"), policy).unwrap();
    let out = assay(dir, &["migrate", "--config", "eval.yaml"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("root JSON Schema keywords"));
    assert_eq!(
        std::fs::read_to_string(dir.join("eval.yaml")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("policy.yaml")).unwrap(),
        policy
    );
    assert!(!dir.join("eval.yaml.bak").exists());
}

#[test]
fn migrated_config_must_fit_the_readers_limit_before_any_write() {
    let template = format!(
        "{}    metadata: {{note: x}}\n",
        eval_yaml("      type: args_valid\n      schema: {Search: {type: object}}")
    );
    let migrate = |dir: &Path, opts: &[&str]| {
        let mut args = vec!["migrate", "--config", "eval.yaml"];
        args.extend_from_slice(opts);
        assay(dir, &args)
    };

    // Measure the formatting overhead with the actual serializer.
    let seed = tempfile::tempdir().unwrap();
    let config = seed.path().join("eval.yaml");
    std::fs::write(&config, &template).unwrap();
    assert!(migrate(seed.path(), &[]).status.success());
    let seed_len = std::fs::read(&config).unwrap().len();
    for excess in [0, 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("eval.yaml");
        let original = template.replace(
            "note: x",
            &format!("note: {}", "x".repeat(1 + 1024 * 1024 + excess - seed_len)),
        );
        std::fs::write(&path, &original).unwrap();
        let preview = migrate(dir.path(), &["--dry-run"]);
        assert_eq!(preview.status.success(), excess == 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!path.with_extension("yaml.bak").exists());
        let output = migrate(dir.path(), &[]);
        if excess == 0 {
            assert!(output.status.success(), "{}", describe(&output));
            assert_eq!(std::fs::metadata(&path).unwrap().len(), 1024 * 1024);
            assert!(migrate(dir.path(), &["--check"]).status.success());
        } else {
            assert!(!output.status.success());
            assert!(
                text(&output).contains("1048576-byte ceiling"),
                "{}",
                describe(&output)
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            assert!(!path.with_extension("yaml.bak").exists());
        }
    }
}

#[test]
fn migrating_legacy_argument_schemas_keeps_unlisted_tools_denied() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let config = eval_yaml("      type: args_valid\n      policy: policy.yaml");
    let policy = "version: 1.0\nname: 18446744073709551616\ntools:\n  arg_constraints:\n    $defs:\n      Pages: {type: integer, minimum: 1, maximum: 3}\n    Search:\n      type: object\n      properties:\n        pages: {$ref: '#/$defs/Pages'}\n";
    std::fs::write(dir.join("eval.yaml"), &config).unwrap();
    std::fs::write(dir.join("policy.yaml"), policy).unwrap();

    let migrated = assay(dir, &["migrate", "--config", "eval.yaml"]);
    assert!(migrated.status.success(), "{}", text(&migrated));
    assert_eq!(
        std::fs::read_to_string(dir.join("eval.yaml.bak")).unwrap(),
        config
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("policy.yaml")).unwrap(),
        policy
    );
    let converted: Value =
        serde_yaml::from_str(&std::fs::read_to_string(dir.join("eval.yaml")).unwrap()).unwrap();
    let schema = &converted["tests"][0]["expected"]["schema"];
    assert_eq!(schema["name"], "18446744073709551616");
    assert_eq!(schema["enforcement"]["unconstrained_tools"], "deny");
    std::fs::remove_file(dir.join("policy.yaml")).unwrap();
    assert!(assay(dir, &["migrate", "--config", "eval.yaml", "--check"])
        .status
        .success());

    for (tool, pages, exit) in [
        ("Search", 0, 1),
        ("Search", 1, 0),
        ("Search", 3, 0),
        ("Search", 4, 1),
        ("Other", 1, 1),
        ("$defs", 1, 1),
    ] {
        std::fs::write(
            dir.join("trace.jsonl"),
            trace_jsonl(&[(tool, json!({"pages": pages}))]),
        )
        .unwrap();
        for out in [run(dir), validate_json(dir)] {
            assert_eq!(
                out.status.code(),
                Some(exit),
                "{tool} {pages}: {}",
                text(&out)
            );
        }
    }
}

#[test]
fn migrating_mixed_legacy_argument_schemas_preserves_both_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let config = eval_yaml("      type: args_valid\n      policy: policy.yaml");
    for (extra, field) in [
        ("  deny: [Search]\n", "tools.deny"),
        ("version: \"1.0\"\n", "duplicate member"),
        ("enforcement: {unconstrained_tools: allow}\n", "enforcement"),
    ] {
        let policy = format!(
            "version: \"1.0\"\ntools:\n  arg_constraints: {{Search: {{type: object}}}}\n{extra}"
        );
        std::fs::write(dir.join("eval.yaml"), &config).unwrap();
        std::fs::write(dir.join("policy.yaml"), &policy).unwrap();
        let out = assay(dir, &["migrate", "--config", "eval.yaml"]);
        assert!(!out.status.success());
        let message = text(&out);
        assert!(message.contains(field), "{message}");
        assert_eq!(
            std::fs::read_to_string(dir.join("eval.yaml")).unwrap(),
            config
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("policy.yaml")).unwrap(),
            policy
        );
        assert!(!dir.join("eval.yaml.bak").exists());
    }
}

#[test]
fn migrate_moves_argument_policy_lists_to_tools_in_evaluation_order() {
    let policy = json!({
        "version":"2.0", "allow":["a","x","both"], "deny":["x"],
        "tools":{"allow":["b","y","both"], "deny":["y","x"]},
        "schemas":{"a":true}, "enforcement":{"unconstrained_tools":"warn"}
    });
    for external in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        std::fs::write(dir.join("policy.yaml"), policy.to_string()).unwrap();
        let expected = if external {
            "      type: args_valid\n      policy: policy.yaml".to_owned()
        } else {
            format!("      type: args_valid\n      schema: {policy}")
        };
        let file = dir.join("eval.yaml");
        std::fs::write(&file, eval_yaml(&expected)).unwrap();
        let verify_calls = || {
            for (tool, code) in [("a", 0), ("b", 0), ("x", 1), ("y", 1), ("other", 1)] {
                std::fs::write(dir.join("trace.jsonl"), trace_jsonl(&[(tool, json!({}))])).unwrap();
                let result = validate_json(dir);
                assert_eq!(
                    result.status.code(),
                    Some(code),
                    "{tool}: {}",
                    describe(&result)
                );
            }
        };
        if external {
            assert_eq!(validate_json(dir).status.code(), Some(2));
        } else {
            verify_calls();
        }
        let migrated = assay(dir, &["migrate", "--config", "eval.yaml"]);
        assert!(migrated.status.success(), "{}", describe(&migrated));
        let doc: serde_json::Value =
            serde_yaml::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            doc["tests"][0]["expected"]["schema"],
            json!({
                "version":"2.0","tools":{"allow":["a","x","both","b","y","both"],"deny":["x","y","x"]},
                "schemas":{"a":true},"enforcement":{"unconstrained_tools":"warn"}
            }),
            "external={external}"
        );
        assert!(assay(dir, &["migrate", "--check", "--config", "eval.yaml"])
            .status
            .success());
        verify_calls();
    }
}

#[test]
fn migrating_an_inline_tool_map_preserves_an_existing_backup() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let original = eval_yaml("      type: args_valid\n      schema: {Search: {type: object}}");
    let config = dir.join("eval.yaml");
    let backup = dir.join("eval.yaml.bak");
    std::fs::write(&config, &original).unwrap();
    std::fs::write(&backup, b"previous migration").unwrap();
    let output = assay(dir, &["migrate", "--config", "eval.yaml"]);
    assert!(!output.status.success());
    assert!(text(&output).contains("eval.yaml.bak"));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
    assert_eq!(std::fs::read(&backup).unwrap(), b"previous migration");
}
