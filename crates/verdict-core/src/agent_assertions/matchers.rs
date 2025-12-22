use super::model::TraceAssertion;
use super::EpisodeGraph;
use crate::errors::diagnostic::Diagnostic;
// usage of HashMap removed

pub fn evaluate(
    graph: &EpisodeGraph,
    assertions: &[TraceAssertion],
) -> anyhow::Result<Vec<Diagnostic>> {
    let mut out = vec![];
    for a in assertions {
        if let Some(d) = check_one(graph, a) {
            out.push(d);
        }
    }
    Ok(out)
}

fn check_one(graph: &EpisodeGraph, a: &TraceAssertion) -> Option<Diagnostic> {
    match a {
        TraceAssertion::TraceMustCallTool { tool, min_calls } => {
            let actual = graph
                .tool_calls
                .iter()
                .filter(|t| t.tool_name.as_deref() == Some(tool.as_str()))
                .count();
            let min = min_calls.unwrap_or(1);
            if (actual as u32) < min {
                return Some(make_diag(
                    "E_TRACE_ASSERT_FAIL",
                    &format!(
                        "Expected tool '{}' to be called at least {} times, but got {}.",
                        tool, min, actual
                    ),
                    Some(format!("Must call tool: {}", tool)),
                    None,
                ));
            }
        }
        TraceAssertion::TraceMustNotCallTool { tool } => {
            if let Some(call) = graph
                .tool_calls
                .iter()
                .find(|t| t.tool_name.as_deref() == Some(tool.as_str()))
            {
                return Some(make_diag(
                    "E_TRACE_ASSERT_FAIL",
                    &format!(
                        "Expected tool '{}' NOT to be called, but it was called.",
                        tool
                    ),
                    Some(format!("Must not call tool: {}", tool)),
                    Some(serde_json::json!({
                        "failing_step_id": call.step_id,
                        "failing_tool": tool,
                        "failing_call_index": call.call_index
                    })),
                ));
            }
        }
        TraceAssertion::TraceToolSequence {
            sequence,
            allow_other_tools,
        } => {
            if *allow_other_tools {
                // Subsequence check
                if let Err(msg) = check_subsequence(&graph.tool_calls, sequence) {
                    return Some(make_diag(
                        "E_TRACE_ASSERT_FAIL",
                        &msg,
                        Some(format!("Tool sequence (subsequence): {:?}", sequence)),
                        None,
                    ));
                }
            } else {
                // Exact sequence check (contiguous, no extras)
                let actual_seq: Vec<String> = graph
                    .tool_calls
                    .iter()
                    .filter_map(|t| t.tool_name.clone())
                    .collect();

                if actual_seq != *sequence {
                    return Some(make_diag(
                        "E_TRACE_ASSERT_FAIL",
                        &format!(
                            "Expected exact tool sequence {:?}, got {:?}.",
                            sequence, actual_seq
                        ),
                        Some(format!("Tool sequence (exact): {:?}", sequence)),
                        None,
                    ));
                }
            }
        }
        TraceAssertion::TraceMaxSteps { max } => {
            let count = graph.steps.len();
            if count as u32 > *max {
                return Some(make_diag(
                    "E_TRACE_ASSERT_FAIL",
                    &format!("Expected at most {} steps, got {}.", max, count),
                    Some(format!("Max steps: {}", max)),
                    None,
                ));
            }
        }
    }
    None
}

fn check_subsequence(
    calls: &[crate::storage::rows::ToolCallRow],
    expected: &[String],
) -> Result<(), String> {
    let mut call_iter = calls.iter();
    // We need to find expected items in order in the call_iter
    // But simplistic iterator matching is not enough if we want to skip non-matching items?
    // "subsequence" usually means they appear in that order, but potentially with gaps.
    // Yes: [A, B] matches [A, X, B].

    // We can't just consume the iterator once strictly if we want flexibility,
    // but actually for subsequence we just search forward.

    let mut current_idx = 0; // index in calls

    for expected_tool in expected {
        // Find next occurrence of expected_tool starting from current_idx
        let mut found = false;
        while current_idx < calls.len() {
            let row = &calls[current_idx];
            current_idx += 1;
            if row.tool_name.as_deref() == Some(expected_tool.as_str()) {
                found = true;
                break;
            }
        }

        if !found {
            return Err(format!(
                "Expected tool '{}' in sequence, but not found (missing or out of order).",
                expected_tool
            ));
        }
    }
    Ok(())
}

fn make_diag(
    code: &str,
    message: &str,
    _expected: Option<String>,
    context: Option<serde_json::Value>,
) -> Diagnostic {
    // We construct Diagnostic manually to match the struct definition.
    // Note: DiagnosticCode enum usage is available in other files but here we might need strings?
    // The Diagnostic struct uses String for code.

    let mut d = Diagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        source: "agent_assertions".to_string(),
        message: message.to_string(),
        context: context.unwrap_or(serde_json::json!({})),
        fix_steps: vec![],
    };

    // We could add expected info to context if not already there
    // But struct doesn't have a dedicated expected field.
    d
}
