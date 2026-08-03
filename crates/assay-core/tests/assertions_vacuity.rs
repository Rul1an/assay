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
