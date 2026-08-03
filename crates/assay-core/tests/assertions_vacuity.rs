//! Assertions that cannot check anything must not report as satisfied.
//!
//! `AGENTS.md`, Development Discipline: "Never turn absence of evidence, failed validation,
//! skipped review, or unavailable infrastructure into a clean result." The `assertions:`
//! evaluator does exactly that in several places — a missing field takes an `if let Some(..)`
//! with no `else` and the assertion contributes no diagnostic, which is indistinguishable from
//! a check that ran and held.
//!
//! Each test below names the mechanism at file:line as of the tree these were written against,
//! and asserts the behaviour we want rather than the behaviour we have. Issue #1949.

use assay_core::agent_assertions::{model::TraceAssertion, verify_assertions};
use assay_core::storage::Store;
use assay_core::trace::schema::{EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use serde_json::json;

/// A store carrying one episode with a single `web_search` tool call, so every assertion below
/// is evaluated against a real episode graph rather than the unit-test fallback.
fn store_with_one_call() -> anyhow::Result<(Store, i64, &'static str)> {
    let store = Store::memory()?;
    store.init_schema()?;
    let run_id = store.insert_run("vacuity-suite")?;
    let test_id = "agent-under-test";

    store.insert_event(
        &TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: "ep-1".into(),
            timestamp: 1000,
            input: json!({ "prompt": "hi" }),
            meta: json!({}),
        }),
        Some(run_id),
        Some(test_id),
    )?;
    store.insert_event(
        &TraceEvent::Step(StepEntry {
            episode_id: "ep-1".into(),
            step_id: "s-1".into(),
            idx: 0,
            timestamp: 1001,
            kind: "tool".into(),
            name: Some("model".into()),
            content: None,
            content_sha256: None,
            truncations: vec![],
            meta: json!({}),
        }),
        Some(run_id),
        Some(test_id),
    )?;
    store.insert_event(
        &TraceEvent::ToolCall(ToolCallEntry {
            episode_id: "ep-1".into(),
            step_id: "s-1".into(),
            timestamp: 1002,
            tool_name: "web_search".into(),
            call_index: Some(0),
            args: json!({ "q": "rust" }),
            args_sha256: None,
            result: None,
            result_sha256: None,
            error: None,
            truncations: vec![],
        }),
        Some(run_id),
        Some(test_id),
    )?;

    Ok((store, run_id, test_id))
}

/// Every assertion that cannot check anything reports through this code, so a suite can tell
/// "nothing to check" apart from "checked and held" without reading prose.
const INEFFECTIVE: &str = "E_ASSERT_INEFFECTIVE";

/// Asserts the diagnostic exists **and** names the field actually responsible.
///
/// Checking only the code is too weak: several ineffective paths sit behind one another, so a
/// diagnostic naming the wrong field still satisfies a code-only assertion while sending the
/// author to look at a field they believe they already supplied. A mutation that removed the
/// typed-`test_trace` guard survived a code-only assertion for exactly that reason.
fn assert_reports_ineffective(
    diags: &[assay_core::errors::diagnostic::Diagnostic],
    case: &str,
    expected_field: &str,
) {
    assert!(
        !diags.is_empty(),
        "{case}: assertion evaluated to nothing and reported no diagnostic, \
         which is indistinguishable from a check that ran and held"
    );
    let found = diags
        .iter()
        .find(|d| d.code == INEFFECTIVE)
        .unwrap_or_else(|| {
            panic!(
                "{case}: expected a {INEFFECTIVE} diagnostic, got {:?}",
                diags.iter().map(|d| &d.code).collect::<Vec<_>>()
            )
        });
    let named = found.context.get("field").and_then(|v| v.as_str());
    assert_eq!(
        named,
        Some(expected_field),
        "{case}: diagnostic blamed the wrong field, which points the author at the wrong fix"
    );
}

/// `matchers.rs:113` — `if let Some(args) = test_args` with no `else`. Against a real episode
/// there is no trace-mode implementation at all, so the assertion silently contributes nothing.
#[test]
fn args_valid_without_test_args_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ArgsValid {
            tool: "web_search".into(),
            test_args: None,
            policy: Some(json!({ "type": "object", "required": ["q"] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(&diags, "args_valid without test_args", "test_args");
    Ok(())
}

/// `matchers.rs:114-121` already diagnoses a missing policy in unit-test mode. Positive control:
/// this must keep working, so the fix does not trade one silent path for another.
#[test]
fn args_valid_with_test_args_and_no_policy_still_diagnoses() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ArgsValid {
            tool: "web_search".into(),
            test_args: Some(json!({ "q": "rust" })),
            policy: None,
            expect: None,
        }],
    )?;
    assert!(
        !diags.is_empty(),
        "a unit-mode args_valid without a policy must stay diagnosed"
    );
    Ok(())
}

/// `matchers.rs:191` — `pol.get("regex")...unwrap_or(".*")`. A policy with no `regex` key becomes
/// a universally permissive check rather than an error, so no trace can ever fail it.
#[test]
fn sequence_valid_policy_without_regex_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::SequenceValid {
            test_trace: None,
            test_trace_raw: Some(vec![json!({ "tool": "web_search" })]),
            policy: Some(json!({ "rules": ["whatever the author meant"] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "sequence_valid policy without regex",
        "policy.regex",
    );
    Ok(())
}

/// `matchers.rs:151-157` — the typed `test_trace` field is destructured away with `..` and never
/// read; only `test_trace_raw` is. A config written against the typed field checks nothing.
#[test]
fn sequence_valid_with_typed_test_trace_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::SequenceValid {
            test_trace: Some(vec![]),
            test_trace_raw: None,
            policy: Some(json!({ "regex": "^web_search$" })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "sequence_valid using the typed test_trace field",
        "test_trace",
    );
    Ok(())
}

/// `matchers.rs:222-230` — a policy with no `blocked` key yields an empty list through
/// `unwrap_or_default()`, and an empty blocklist admits every tool call.
#[test]
fn tool_blocklist_without_blocked_key_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ToolBlocklist {
            test_tool_calls: Some(vec!["web_search".into()]),
            policy: Some(json!({ "deny": ["rm"] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "tool_blocklist policy without a blocked key",
        "policy.blocked",
    );
    Ok(())
}

/// Same shape, stated explicitly rather than through a missing key.
#[test]
fn tool_blocklist_with_empty_blocked_list_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ToolBlocklist {
            test_tool_calls: Some(vec!["web_search".into()]),
            policy: Some(json!({ "blocked": [] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(&diags, "tool_blocklist with blocked: []", "policy.blocked");
    Ok(())
}

/// `matchers.rs:219-220` — `test_tool_calls` absent takes the outer `if let Some` and the whole
/// assertion evaluates to nothing.
#[test]
fn tool_blocklist_without_test_tool_calls_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ToolBlocklist {
            test_tool_calls: None,
            policy: Some(json!({ "blocked": ["rm"] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "tool_blocklist without test_tool_calls",
        "test_tool_calls",
    );
    Ok(())
}

/// `expect` was compared by exact string equality to `"pass"`, so any other spelling silently
/// meant *expect failure* and inverted the assertion. An author writing `expect: Pass` got a test
/// that goes green precisely when the policy would have rejected the input.
///
/// This is worse than a no-op: a no-op stops checking, an inversion checks the opposite.
///
/// Covers **all three** call sites. Testing one of them was not enough: an adversarial review
/// restored the pre-fix comparison at the `sequence_valid` and `tool_blocklist` sites and every
/// test in the crate stayed green, because only `args_valid` was exercised here.
#[test]
fn unrecognised_expect_value_is_rejected_not_silently_inverted() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    for spelling in ["Pass", "PASS", "passes", "true", "ok", ""] {
        for (site, assertion) in expect_sites(spelling) {
            let diags = verify_assertions(&store, run_id, test_id, &[assertion])?;
            assert!(
                diags.iter().any(|d| d.code == "E_CONFIG_ERROR"),
                "{site} with expect: {spelling:?} must be rejected, not read as \
                 `expect failure`; got {:?}",
                diags.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }
    Ok(())
}

/// One well-formed assertion per variant that reads `expect`, so the polarity parser is pinned
/// everywhere it is called rather than only where it was convenient to test.
fn expect_sites(spelling: &str) -> Vec<(&'static str, TraceAssertion)> {
    let expect = Some(spelling.to_string());
    vec![
        (
            "args_valid",
            TraceAssertion::ArgsValid {
                tool: "web_search".into(),
                test_args: Some(json!({ "q": "rust" })),
                policy: Some(json!({ "schema": { "type": "object", "required": ["q"] } })),
                expect: expect.clone(),
            },
        ),
        (
            "sequence_valid",
            TraceAssertion::SequenceValid {
                test_trace: None,
                test_trace_raw: Some(vec![json!({ "tool": "web_search" })]),
                policy: Some(json!({ "regex": "^web_search$" })),
                expect: expect.clone(),
            },
        ),
        (
            "tool_blocklist",
            TraceAssertion::ToolBlocklist {
                test_tool_calls: Some(vec!["web_search".into()]),
                policy: Some(json!({ "blocked": ["rm"] })),
                expect,
            },
        ),
    ]
}

/// Both recognised spellings must keep working, so rejecting the rest does not break real configs.
#[test]
fn recognised_expect_values_still_work() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let schema = json!({ "schema": { "type": "object", "required": ["q"] } });

    // `pass` against args that satisfy the schema: holds, so silent.
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ArgsValid {
            tool: "web_search".into(),
            test_args: Some(json!({ "q": "rust" })),
            policy: Some(schema.clone()),
            expect: Some("pass".into()),
        }],
    )?;
    assert!(diags.is_empty(), "expect: pass on valid args must hold");

    // `fail` against args that violate the schema: also holds, so also silent.
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ArgsValid {
            tool: "web_search".into(),
            test_args: Some(json!({ "wrong": 1 })),
            policy: Some(schema),
            expect: Some("fail".into()),
        }],
    )?;
    assert!(diags.is_empty(), "expect: fail on invalid args must hold");
    Ok(())
}

/// `min_calls: 0` makes `(actual as u32) < 0` — never true for an unsigned count, so the
/// assertion cannot fail for any trace.
#[test]
fn must_call_tool_with_min_calls_zero_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMustCallTool {
            tool: "web_search".into(),
            min_calls: Some(0),
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "trace_must_call_tool with min_calls: 0",
        "min_calls",
    );
    Ok(())
}

/// An empty sequence under `allow_other_tools: true` is a subsequence check with nothing to
/// find, which every trace satisfies.
#[test]
fn tool_sequence_empty_subsequence_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceToolSequence {
            sequence: vec![],
            allow_other_tools: true,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "trace_tool_sequence with an empty sequence and allow_other_tools",
        "sequence",
    );
    Ok(())
}

/// The exact-sequence form of an empty sequence is *not* vacuous — it asserts the trace made no
/// named tool call, which this trace violates. Positive control against over-rejecting.
#[test]
fn tool_sequence_empty_exact_is_a_real_constraint() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceToolSequence {
            sequence: vec![],
            allow_other_tools: false,
        }],
    )?;
    assert!(
        diags.iter().any(|d| d.code == "E_TRACE_ASSERT_FAIL"),
        "an empty exact sequence constrains the trace to no tool calls and must fail here, got {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    Ok(())
}

/// A policy whose `regex` is present but cannot constrain anything — empty pattern, or a
/// non-string that previously degraded to `.*`.
#[test]
fn sequence_valid_permissive_regex_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    for pol in [json!({ "regex": "" }), json!({ "regex": 123 })] {
        let diags = verify_assertions(
            &store,
            run_id,
            test_id,
            &[TraceAssertion::SequenceValid {
                test_trace: None,
                test_trace_raw: Some(vec![json!({ "tool": "web_search" })]),
                policy: Some(pol.clone()),
                expect: None,
            }],
        )?;
        assert_reports_ineffective(
            &diags,
            &format!("sequence_valid regex {pol}"),
            "policy.regex",
        );
    }
    Ok(())
}

/// A `blocked` value that is not an array of strings collapses to an empty blocklist, which
/// admits every call.
#[test]
fn tool_blocklist_unusable_blocked_value_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    for pol in [
        json!({ "blocked": "rm" }),
        json!({ "blocked": [42] }),
        json!({ "blocked": [{ "name": "rm" }] }),
    ] {
        let diags = verify_assertions(
            &store,
            run_id,
            test_id,
            &[TraceAssertion::ToolBlocklist {
                test_tool_calls: Some(vec!["web_search".into()]),
                policy: Some(pol.clone()),
                expect: None,
            }],
        )?;
        assert_reports_ineffective(
            &diags,
            &format!("tool_blocklist blocked {pol}"),
            "policy.blocked",
        );
    }
    Ok(())
}

/// The structural twin of `blocked: []`, from the other side: with no calls to check, the loop
/// starts at "passing" and never iterates, so no blocklist can fail it. Rejecting one side of the
/// pair and not the other would be an arbitrary boundary.
#[test]
fn tool_blocklist_with_no_calls_to_check_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ToolBlocklist {
            test_tool_calls: Some(vec![]),
            policy: Some(json!({ "blocked": ["rm"] })),
            expect: Some("pass".into()),
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "tool_blocklist with an empty test_tool_calls",
        "test_tool_calls",
    );
    Ok(())
}

/// A partly-unusable blocklist is worse than a wholly unusable one: the assertion keeps working
/// for the entries it could read, so it looks complete while silently checking less.
#[test]
fn tool_blocklist_partially_unusable_blocked_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::ToolBlocklist {
            test_tool_calls: Some(vec!["drop_table".into()]),
            policy: Some(json!({ "blocked": ["rm", { "name": "drop_table" }] })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "tool_blocklist with a non-string entry in blocked",
        "policy.blocked",
    );
    Ok(())
}

/// Same shape one variant over: an entry keyed on neither `tool` nor `tool_name` is dropped, so
/// the sequence actually checked is shorter than the one written.
#[test]
fn sequence_valid_unreadable_trace_entry_is_not_a_pass() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::SequenceValid {
            test_trace: None,
            test_trace_raw: Some(vec![
                json!({ "tool": "web_search" }),
                json!({ "toolName": "delete_account" }),
            ]),
            policy: Some(json!({ "regex": "^web_search$" })),
            expect: None,
        }],
    )?;
    assert_reports_ineffective(
        &diags,
        "sequence_valid with an entry naming no tool",
        "test_trace_raw",
    );
    Ok(())
}

/// Negative control: a fully specified assertion that genuinely holds must stay silent, or the
/// fix has simply made everything noisy.
#[test]
fn effective_assertion_that_holds_stays_silent() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMustCallTool {
            tool: "web_search".into(),
            min_calls: Some(1),
        }],
    )?;
    assert!(
        diags.is_empty(),
        "an effective assertion that holds must report nothing, got {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    Ok(())
}

/// Negative control on the other side: a real violation must still be reported as a failure,
/// not reclassified as ineffective.
#[test]
fn effective_assertion_that_fails_still_fails() -> anyhow::Result<()> {
    let (store, run_id, test_id) = store_with_one_call()?;
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMustNotCallTool {
            tool: "web_search".into(),
        }],
    )?;
    assert!(
        diags.iter().any(|d| d.code == "E_TRACE_ASSERT_FAIL"),
        "a genuine violation must stay a failure, got {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    Ok(())
}
