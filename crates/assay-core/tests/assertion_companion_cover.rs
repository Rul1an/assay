//! The headline case of #1949, end to end through the store (layer 2, assertion half).
//!
//! `trace_must_not_call_tool` naming a tool the agent never had is syntactically perfect,
//! config-effective — `ineffective_reason` passes it, correctly, because a trace that called the
//! tool would fail it — and permanently vacuous. Neither the static sweep nor the metric-side cover
//! could reach it: assertions produce `Diagnostic`s, not `MetricResult`s.
//!
//! These go through a real `Store` and the real event ingest rather than a hand-built
//! `EpisodeGraph`, so they exercise `verify_assertions_with_meta` on the shape the runner passes.

use assay_core::agent_assertions::{model::TraceAssertion, verify_assertions_with_meta};
use assay_core::storage::Store;
use assay_core::trace::schema::{EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use serde_json::json;

/// A store holding one episode: one step, and one call to each tool in `calls`.
fn seeded_store(calls: &[&str]) -> anyhow::Result<(Store, i64, &'static str)> {
    let store = Store::memory()?;
    store.init_schema()?;
    let run_id = store.insert_run("cover-suite")?;
    let test_id = "agent-under-test";

    store.insert_event(
        &TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: "ep-1".into(),
            timestamp: 1000,
            input: json!({"prompt": "do the thing"}),
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
    for (i, tool) in calls.iter().enumerate() {
        store.insert_event(
            &TraceEvent::ToolCall(ToolCallEntry {
                episode_id: "ep-1".into(),
                step_id: "s-1".into(),
                timestamp: 1002 + i as u64,
                tool_name: (*tool).into(),
                call_index: Some(i as u32),
                args: json!({}),
                args_sha256: None,
                result: None,
                result_sha256: None,
                error: None,
                truncations: vec![],
            }),
            Some(run_id),
            Some(test_id),
        )?;
    }
    Ok((store, run_id, test_id))
}

fn tool_definitions(names: &[&str]) -> serde_json::Value {
    json!({
        "tool_definitions": names.iter().map(|n| json!({"name": n})).collect::<Vec<_>>()
    })
}

fn guard(tool: &str) -> Vec<TraceAssertion> {
    vec![TraceAssertion::TraceMustNotCallTool { tool: tool.into() }]
}

/// The case the whole layer exists for: the assertion passes, and it never could have failed.
#[test]
fn a_guard_on_a_tool_the_agent_never_had_passes_and_is_reported_as_unexercised(
) -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["read_file"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
        &tool_definitions(&["read_file", "list_dir"]),
    )?;

    assert!(
        outcome.diagnostics.is_empty(),
        "the assertion holds: nothing failed"
    );
    assert_eq!(outcome.not_exercised.len(), 1, "and it held vacuously");
    assert_eq!(
        outcome.not_exercised[0].assertion,
        "trace_must_not_call_tool"
    );
    assert!(
        outcome.not_exercised[0].reason.contains("never offered"),
        "{}",
        outcome.not_exercised[0].reason
    );
    Ok(())
}

/// The pass that was earned. Reporting this would fire on every working guard in every suite, which
/// is how a vacuity check earns its suppression.
#[test]
fn a_guard_on_an_available_tool_the_agent_declined_is_silent() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["read_file"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
        &tool_definitions(&["read_file", "delete_repository"]),
    )?;

    assert!(outcome.diagnostics.is_empty());
    assert!(
        outcome.not_exercised.is_empty(),
        "the agent had the tool and left it alone; that is a real pass"
    );
    Ok(())
}

/// A trace with no tool list recorded says nothing about availability, and this must stay silent.
///
/// Most traces are this. Treating "unrecorded" as "absent" would put a finding on every
/// `trace_must_not_call_tool` in every suite that replays a plain trace.
#[test]
fn a_trace_without_a_recorded_tool_list_is_silent() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["read_file"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
        &json!({}),
    )?;

    assert!(outcome.diagnostics.is_empty());
    assert!(outcome.not_exercised.is_empty());
    Ok(())
}

/// A guard that actually caught something is a failure, and a failure is never also a coverage
/// hole: the tool was called, so it plainly existed.
#[test]
fn a_guard_that_fires_is_a_failure_and_not_a_cover() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["delete_repository"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
        // Deliberately *not* declared, to prove a call alone establishes availability.
        &json!({}),
    )?;

    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].code, "E_TRACE_ASSERT_FAIL");
    assert!(outcome.not_exercised.is_empty());
    Ok(())
}

/// A tool that was called but is missing from the declared list is still available.
///
/// The declared list can be incomplete — a proxy that recorded a partial `tools/list`, a tool
/// injected later in the episode. Trusting it over the observed call would report the *opposite* of
/// the truth: that the agent never had a tool it demonstrably used.
#[test]
fn a_called_tool_missing_from_the_declared_list_is_not_a_cover() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["delete_repository"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        // A guard on a *different* tool, so the guard itself passes and the only question is what
        // the availability of `read_file` is judged to be.
        &guard("read_file"),
        // `read_file` is declared; `delete_repository` was called and is not. The call is what
        // settles the second one.
        &tool_definitions(&["read_file"]),
    )?;
    assert!(
        outcome.not_exercised.is_empty(),
        "{:?}",
        outcome.not_exercised
    );

    // And the reverse: a guard on the called-but-undeclared tool must not read as unexercised.
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
        &tool_definitions(&["read_file"]),
    )?;
    assert_eq!(
        outcome.diagnostics.len(),
        1,
        "it was called, so the guard fires"
    );
    assert!(
        outcome.not_exercised.is_empty(),
        "a tool the agent demonstrably used was reported as never offered: {:?}",
        outcome.not_exercised
    );
    Ok(())
}

/// One test can fail one assertion and never exercise another. Both are reported.
#[test]
fn a_failing_assertion_does_not_hide_an_unexercised_one() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["read_file"])?;
    let outcome = verify_assertions_with_meta(
        &store,
        run_id,
        test_id,
        &[
            TraceAssertion::TraceMustCallTool {
                tool: "write_file".into(),
                min_calls: Some(1),
            },
            TraceAssertion::TraceMustNotCallTool {
                tool: "delete_repository".into(),
            },
        ],
        &tool_definitions(&["read_file"]),
    )?;

    assert_eq!(outcome.diagnostics.len(), 1, "write_file was never called");
    assert_eq!(
        outcome.not_exercised.len(),
        1,
        "and the guard never guarded"
    );
    Ok(())
}

/// The four-argument entry point keeps working and reports no covers, which is the honest answer
/// for a caller that has no response metadata to give.
#[test]
fn the_metadata_free_entry_point_still_returns_the_diagnostics() -> anyhow::Result<()> {
    let (store, run_id, test_id) = seeded_store(&["delete_repository"])?;
    let diags = assay_core::agent_assertions::verify_assertions(
        &store,
        run_id,
        test_id,
        &guard("delete_repository"),
    )?;
    assert_eq!(diags.len(), 1);
    Ok(())
}

/// Layer 3 passes this config, and it must: a trace that called the tool would fail it. Pinned so
/// nobody "fixes" the static sweep to catch it, which would reject working guards.
#[test]
fn the_static_sweep_correctly_declines_to_flag_this_config() {
    let a = TraceAssertion::TraceMustNotCallTool {
        tool: "delete_repository".into(),
    };
    assert!(
        assay_core::agent_assertions::matchers::ineffective_reason(&a).is_none(),
        "layer 3 flagged a config-effective guard; layer 2 is what covers this case"
    );
}
