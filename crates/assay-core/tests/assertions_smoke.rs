use assay_core::agent_assertions::{model::TraceAssertion, verify_assertions};
use assay_core::storage::Store;
use assay_core::trace::schema::{EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use serde_json::json;

#[test]
fn test_assertions_logic() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?; // Includes FK PRAGMA

    // Insert Parent Run (FK Requirement)
    let run_id = store.insert_run("test-suite")?; // Removed None arg
    let test_id = "test-agent";

    // Seed Episode
    let ep = EpisodeStart {
        episode_id: "ep-1".into(),
        timestamp: 1000,
        input: json!({"prompt": "hi"}),
        meta: json!({}),
    };
    store.insert_event(
        &TraceEvent::EpisodeStart(ep.clone()),
        Some(run_id),
        Some(test_id),
    )?;

    // Step 1: Tool Call (web_search)
    let step1 = StepEntry {
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
    };
    store.insert_event(&TraceEvent::Step(step1), Some(run_id), Some(test_id))?;

    let tc1 = ToolCallEntry {
        episode_id: "ep-1".into(),
        step_id: "s-1".into(),
        timestamp: 1002,
        tool_name: "web_search".into(),
        call_index: Some(0),
        args: json!({"q": "rust"}),
        args_sha256: None,
        result: None,
        result_sha256: None,
        error: None,
        truncations: vec![],
    };
    store.insert_event(&TraceEvent::ToolCall(tc1), Some(run_id), Some(test_id))?;

    // ASSERTION 1: Must Call 'web_search' -> PASS
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMustCallTool {
            tool: "web_search".into(),
            min_calls: None,
        }],
    )?;
    assert!(
        diags.is_empty(),
        "Expected no failure for must_call(web_search)"
    );

    // ASSERTION 2: Must NOT Call 'calculator' -> PASS
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMustNotCallTool {
            tool: "calculator".into(),
        }],
    )?;
    assert!(
        diags.is_empty(),
        "Expected no failure for must_not_call(calculator)"
    );

    // ASSERTION 3: Tool Sequence (Exact) -> FAIL (missing item)
    // Actual: [web_search]
    // Expected: [web_search, summarize] (exact)
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceToolSequence {
            sequence: vec!["web_search".into(), "summarize".into()],
            allow_other_tools: false,
        }],
    )?;
    assert_eq!(
        diags.len(),
        1,
        "Expected 1 failure for sequence exact mismatch"
    );
    assert!(
        diags[0].message.contains("Expected exact tool sequence"),
        "Msg mismatch: {}",
        diags[0].message
    );

    // ASSERTION 4: Tool Sequence (Subsequence) -> FAIL (missing item)
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceToolSequence {
            sequence: vec!["web_search".into(), "summarize".into()],
            allow_other_tools: true,
        }],
    )?;
    assert_eq!(
        diags.len(),
        1,
        "Expected 1 failure for sequence subsequence mismatch"
    );
    assert!(
        diags[0]
            .message
            .contains("Expected tool 'summarize' in sequence, but not found"),
        "Msg mismatch: {}",
        diags[0].message
    );

    // ASSERTION 5: Max Steps -> PASS (1 step <= 5)
    let diags = verify_assertions(
        &store,
        run_id,
        test_id,
        &[TraceAssertion::TraceMaxSteps { max: 5 }],
    )?;
    assert!(diags.is_empty());

    Ok(())
}
