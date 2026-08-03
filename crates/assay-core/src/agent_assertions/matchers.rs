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
        TraceAssertion::ArgsValid {
            tool,
            test_args,
            policy,
            expect,
        } => {
            let Some(args) = test_args else {
                // No trace-mode evaluation exists for this variant, so without `test_args` there
                // is nothing to check. Reporting nothing here would be indistinguishable from a
                // check that ran and held.
                return Some(ineffective(
                    "args_valid",
                    "test_args",
                    "`args_valid` is only evaluated against the arguments supplied in `test_args`; \
                     with that field absent the assertion checks nothing.",
                    "Give the assertion `test_args`, or drop it and constrain the trace with \
                     `trace_must_call_tool` or `trace_tool_sequence`.",
                ));
            };
            {
                let Some(pol) = policy else {
                    return Some(make_diag(
                        "E_CONFIG_ERROR",
                        "ArgsValid assertion requires 'policy' field (schema) when used in unit test mode.",
                        None,
                        None
                    ));
                };

                // Accommodate structure: { schema: { ... } } vs { properties: ... }
                let schema = pol.get("schema").unwrap_or(pol);
                // Wrap in tool map as expected by policy_engine
                let policy_map = serde_json::json!({ tool: schema });

                let verdict = crate::policy_engine::evaluate_tool_args(&policy_map, tool, args);
                let expected_pass = expect.as_deref().unwrap_or("pass") == "pass";
                let actual_pass = verdict.status == crate::policy_engine::VerdictStatus::Allowed;

                if expected_pass != actual_pass {
                    return Some(make_diag(
                        "E_POLICY_ASSERT_FAIL",
                        &format!(
                            "ArgsValid check failed. Expected {}, got {}. Reason: {:?}",
                            if expected_pass { "PASS" } else { "FAIL" },
                            if actual_pass { "PASS" } else { "FAIL" },
                            verdict.details
                        ),
                        None,
                        Some(serde_json::json!({
                            "tool": tool,
                            "args": args,
                            "verdict": verdict
                        })),
                    ));
                }
            }
        }
        TraceAssertion::SequenceValid {
            test_trace,
            test_trace_raw,
            policy,
            expect,
        } => {
            // The typed `test_trace` field is not evaluated — only `test_trace_raw` is read
            // below. Saying so beats ignoring a field the author believed was carrying the test.
            if test_trace.is_some() && test_trace_raw.is_none() {
                return Some(ineffective(
                    "sequence_valid",
                    "test_trace",
                    "`test_trace` is not evaluated; only `test_trace_raw` is read, so this \
                     assertion checks nothing.",
                    "Move the steps to `test_trace_raw` as a list of `{ tool: <name> }` entries.",
                ));
            }
            let Some(trace_vals) = test_trace_raw else {
                return Some(ineffective(
                    "sequence_valid",
                    "test_trace_raw",
                    "`sequence_valid` is only evaluated against the steps supplied in \
                     `test_trace_raw`; with that field absent the assertion checks nothing.",
                    "Give the assertion `test_trace_raw`, or constrain the recorded trace with \
                     `trace_tool_sequence` instead.",
                ));
            };
            {
                let Some(pol) = policy else {
                    return Some(ineffective(
                        "sequence_valid",
                        "policy",
                        "`sequence_valid` has no policy to evaluate the steps against, so the \
                         assertion checks nothing.",
                        "Give the assertion a `policy` carrying a `regex` field.",
                    ));
                };
                {
                    // Extract tool names from trace
                    // trace_vals is Vec<Value>. Expect { tool_name: "..." }
                    let tools: Vec<String> = trace_vals
                        .iter()
                        .filter_map(|v| {
                            v.get("tool")
                                .or(v.get("tool_name"))
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();

                    // The policy carries the sequence constraint as a regex under `regex`.
                    // Defaulting a missing key to `.*` would make the assertion match every
                    // possible trace, which is a check that cannot fail rather than a check
                    // that passes.
                    let Some(regex) = pol.get("regex").and_then(|s| s.as_str()) else {
                        return Some(ineffective(
                            "sequence_valid",
                            "policy.regex",
                            "the policy carries no `regex` key, so there is no constraint to \
                             evaluate the steps against.",
                            "Add a `regex` field to the policy describing the permitted tool \
                             sequence.",
                        ));
                    };

                    let verdict = crate::policy_engine::evaluate_sequence(regex, &tools);
                    let expected_pass = expect.as_deref().unwrap_or("pass") == "pass";
                    let actual_pass =
                        verdict.status == crate::policy_engine::VerdictStatus::Allowed;

                    if expected_pass != actual_pass {
                        return Some(make_diag(
                            "E_POLICY_ASSERT_FAIL",
                            &format!(
                                "SequenceValid check failed. Expected {}, got {}.",
                                if expected_pass { "PASS" } else { "FAIL" },
                                if actual_pass { "PASS" } else { "FAIL" }
                            ),
                            None,
                            None,
                        ));
                    }
                }
            }
        }
        TraceAssertion::ToolBlocklist {
            test_tool_calls,
            policy,
            expect,
        } => {
            let Some(tools) = test_tool_calls else {
                return Some(ineffective(
                    "tool_blocklist",
                    "test_tool_calls",
                    "`tool_blocklist` is only evaluated against the calls supplied in \
                     `test_tool_calls`; with that field absent the assertion checks nothing.",
                    "Give the assertion `test_tool_calls`, or constrain the recorded trace with \
                     `trace_must_not_call_tool` instead.",
                ));
            };
            {
                let Some(pol) = policy else {
                    return Some(ineffective(
                        "tool_blocklist",
                        "policy",
                        "`tool_blocklist` has no policy to evaluate the calls against, so the \
                         assertion checks nothing.",
                        "Give the assertion a `policy` carrying a `blocked` list.",
                    ));
                };
                {
                    // pol should look like { "blocked": [...] }
                    // An absent key and an empty list are both a blocklist that admits every
                    // call, which cannot fail for any input.
                    let Some(blocked_raw) = pol.get("blocked").and_then(|v| v.as_array()) else {
                        return Some(ineffective(
                            "tool_blocklist",
                            "policy.blocked",
                            "the policy carries no `blocked` list, so every tool call is \
                             admitted and the assertion cannot fail.",
                            "Add a `blocked` array to the policy naming the disallowed tools.",
                        ));
                    };
                    let blocked: Vec<String> = blocked_raw
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if blocked.is_empty() {
                        return Some(ineffective(
                            "tool_blocklist",
                            "policy.blocked",
                            "the `blocked` list is empty, so every tool call is admitted and the \
                             assertion cannot fail.",
                            "Name at least one disallowed tool in `blocked`, or remove the \
                             assertion.",
                        ));
                    }

                    let expected_pass = expect.as_deref().unwrap_or("pass") == "pass";
                    // Check if *any* tool is blocked
                    let mut actual_pass = true;
                    for t in tools {
                        if blocked.contains(t) {
                            actual_pass = false;
                            break;
                        }
                    }

                    if expected_pass != actual_pass {
                        return Some(make_diag(
                            "E_POLICY_ASSERT_FAIL",
                            &format!(
                                "ToolBlocklist check failed. Expected {}, got {}.",
                                if expected_pass { "PASS" } else { "FAIL" },
                                if actual_pass { "PASS" } else { "FAIL" }
                            ),
                            None,
                            None,
                        ));
                    }
                }
            }
        }
    }
    None
}

fn check_subsequence(
    calls: &[crate::storage::rows::ToolCallRow],
    expected: &[String],
) -> Result<(), String> {
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

/// An assertion that cannot check anything, reported under its own code so a suite can tell it
/// apart from a check that ran and held.
///
/// `AGENTS.md`, Development Discipline: never turn absence of evidence into a clean result. The
/// message names the variant and the field responsible rather than the value, so the diagnostic
/// stays value-free and safe to print.
fn ineffective(variant: &str, field: &str, why: &str, fix: &str) -> Diagnostic {
    Diagnostic {
        code: "E_ASSERT_INEFFECTIVE".to_string(),
        severity: "error".to_string(),
        source: "agent_assertions".to_string(),
        message: format!("Assertion `{variant}` checks nothing: {why}"),
        context: serde_json::json!({ "assertion": variant, "field": field }),
        fix_steps: vec![fix.to_string()],
    }
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

    Diagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        source: "agent_assertions".to_string(),
        message: message.to_string(),
        context: context.unwrap_or(serde_json::json!({})),
        fix_steps: vec![],
    }
}
