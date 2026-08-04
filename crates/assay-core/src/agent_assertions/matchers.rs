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

/// Why this assertion cannot check anything, decided from the configuration alone.
///
/// `Some` means no trace could ever make the assertion fail, so running it would report a pass
/// that carries no information. `None` means the assertion constrains something; whether it then
/// holds is a question about a trace and is not answered here.
///
/// This lets a static sweep — `assay validate`, which reads a config without running it — reach
/// the same conclusions the evaluator reaches, **without a second definition of "cannot fail"**.
/// Two implementations of one rule drift; the fix for that is one implementation with two callers.
/// So this runs the real checker against an empty episode and keeps only the diagnostics that were
/// decided before any trace was consulted.
///
/// That filter is exact rather than approximate, and the reason is a property of the checker:
/// every `E_ASSERT_INEFFECTIVE` and `E_CONFIG_ERROR` in `check_one` is returned from a branch that
/// reads only the assertion's own fields, and every branch that reads the graph returns a
/// different code (`E_TRACE_ASSERT_FAIL`, `E_POLICY_ASSERT_FAIL`).
/// `validate::vacuous_expected_tests::does_not_flag_an_assertion_that_merely_fails_for_a_trace`
/// pins that split, so a future check that broke it fails there rather than silently making the
/// sweep reject working configurations.
///
/// The cost of the reuse is that a valid `args_valid` compiles its schema here as well as at
/// evaluation, and the result is then discarded. In a static sweep that cost is acceptable, but
/// it buys nothing: a schema that cannot compile produces an evaluation-decided code, which this
/// filter drops, so the sweep stays silent about it. That is deliberate — a broken schema is not
/// an assertion that cannot fail — but it is a boundary rather than a feature, and
/// `a_schema_that_cannot_compile_neither_panics_nor_is_swept` pins it so nobody has to rediscover it.
pub fn ineffective_reason(a: &TraceAssertion) -> Option<Diagnostic> {
    let no_trace = EpisodeGraph {
        episode_id: "static_config_sweep".into(),
        steps: vec![],
        tool_calls: vec![],
    };
    check_one(&no_trace, a)
        .filter(|d| d.code == "E_ASSERT_INEFFECTIVE" || d.code == "E_CONFIG_ERROR")
}

fn check_one(graph: &EpisodeGraph, a: &TraceAssertion) -> Option<Diagnostic> {
    match a {
        TraceAssertion::TraceMustCallTool { tool, min_calls } => {
            if let Some(d) = names_no_tool(tool, "trace_must_call_tool", EMPTY_TOOL_NEVER_HOLDS) {
                return Some(d);
            }
            let actual = graph
                .tool_calls
                .iter()
                .filter(|t| t.tool_name.as_deref() == Some(tool.as_str()))
                .count();
            let min = min_calls.unwrap_or(1);
            if min == 0 {
                // `count < 0` is never true for an unsigned count, so no trace can fail this.
                return Some(ineffective(
                    "trace_must_call_tool",
                    "min_calls",
                    "`min_calls: 0` is satisfied by every trace, including one where the tool is \
                     never called.",
                    "Use `min_calls: 1` or higher, or express \"never called\" with \
                     `trace_must_not_call_tool`.",
                ));
            }
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
            if let Some(d) = names_no_tool(tool, "trace_must_not_call_tool", EMPTY_TOOL_NEVER_FAILS)
            {
                return Some(d);
            }
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
                // An empty subsequence has nothing to look for, so every trace contains it.
                // The exact form is different: an empty sequence there asserts the trace made no
                // named tool call at all, which is a real constraint.
                if sequence.is_empty() {
                    return Some(ineffective(
                        "trace_tool_sequence",
                        "sequence",
                        "an empty `sequence` with `allow_other_tools: true` is contained in every \
                         trace, so the assertion cannot fail.",
                        "Name the tools the trace must contain, or set \
                         `allow_other_tools: false` to assert that no tool was called.",
                    ));
                }
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
            // No step count can exceed the ceiling of the type the bound is written in, so this
            // one bound holds for every trace. Only the ceiling is refused: a merely large bound
            // like 100_000 is a real constraint whose outcome depends on the trace, and refusing
            // those is the over-eager vacuity detection that earns a suppression.
            if *max == u32::MAX {
                return Some(ineffective(
                    "trace_max_steps",
                    "max",
                    "`max` is the largest representable bound, so no trace can exceed it and the \
                     assertion cannot fail.",
                    "Set `max` to the step budget the agent is actually expected to stay within.",
                ));
            }
            let count = graph.steps.len();
            // Compare in `usize` rather than casting the count down to `u32`: a count above
            // 2^32 would wrap and could land under the bound.
            if count > *max as usize {
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
                let expected_pass = match expected_pass(expect, "args_valid") {
                    Ok(v) => v,
                    Err(d) => return Some(*d),
                };
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
                    // An entry keyed on neither `tool` nor `tool_name` is dropped above, which
                    // silently shortens the sequence being checked — a misspelled key would turn
                    // this into a different, weaker assertion rather than an error.
                    if tools.len() != trace_vals.len() {
                        return Some(ineffective(
                            "sequence_valid",
                            "test_trace_raw",
                            "an entry in `test_trace_raw` names no tool under `tool` or \
                             `tool_name`, so it is dropped and the sequence checked is shorter \
                             than the one written.",
                            "Give every entry a `tool` key naming one tool.",
                        ));
                    }

                    // The policy carries the sequence constraint as a regex under `regex`.
                    // Defaulting a missing key to `.*` would make the assertion match every
                    // possible trace, which is a check that cannot fail rather than a check
                    // that passes.
                    let regex = pol
                        .get("regex")
                        .and_then(|s| s.as_str())
                        .filter(|r| !r.is_empty());
                    let Some(regex) = regex else {
                        return Some(ineffective(
                            "sequence_valid",
                            "policy.regex",
                            "the policy carries no usable `regex`, so there is no constraint to \
                             evaluate the steps against — an absent, non-string, or empty \
                             pattern matches every possible trace.",
                            "Add a `regex` field to the policy describing the permitted tool \
                             sequence.",
                        ));
                    };

                    let verdict = crate::policy_engine::evaluate_sequence(regex, &tools);
                    let expected_pass = match expected_pass(expect, "sequence_valid") {
                        Ok(v) => v,
                        Err(d) => return Some(*d),
                    };
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
                    // No calls to check is the same shape as an empty blocklist, from the other
                    // side: the loop below starts at `actual_pass = true` and never iterates.
                    if tools.is_empty() {
                        return Some(ineffective(
                            "tool_blocklist",
                            "test_tool_calls",
                            "`test_tool_calls` is empty, so there is no call to match against the \
                             blocklist and the assertion cannot fail.",
                            "List the calls the policy should be evaluated against, or remove the \
                             assertion.",
                        ));
                    }

                    // pol should look like { "blocked": [...] }
                    // An absent key and an empty list are both a blocklist that admits every
                    // call, which cannot fail for any input.
                    let blocked_value = pol.get("blocked");
                    let Some(blocked_raw) = blocked_value.and_then(|v| v.as_array()) else {
                        // Distinguish absent from present-but-wrong-typed: they point the author
                        // at different fixes, and "carries no list" is false for `blocked: "rm"`.
                        return Some(ineffective(
                            "tool_blocklist",
                            "policy.blocked",
                            if blocked_value.is_some() {
                                "the policy's `blocked` is not a list, so no tool name can be \
                                 matched against it and the assertion cannot fail."
                            } else {
                                "the policy carries no `blocked` list, so every tool call is \
                                 admitted and the assertion cannot fail."
                            },
                            "Give `blocked` an array of tool names.",
                        ));
                    };
                    // A non-string entry is dropped by the conversion below. Dropping some of the
                    // blocklist silently would leave a check that looks complete and is not.
                    if blocked_raw.iter().any(|v| !v.is_string()) {
                        return Some(ineffective(
                            "tool_blocklist",
                            "policy.blocked",
                            "`blocked` contains an entry that is not a tool name, which would be \
                             dropped and leave the assertion checking less than it appears to.",
                            "Make every entry in `blocked` a string naming one tool.",
                        ));
                    }
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

                    let expected_pass = match expected_pass(expect, "tool_blocklist") {
                        Ok(v) => v,
                        Err(d) => return Some(*d),
                    };
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

/// An empty `tool` names nothing, and no recorded call carries an empty name.
///
/// Which way that breaks depends on the assertion's polarity — `trace_must_not_call_tool` can then
/// never fail, `trace_must_call_tool` can never be satisfied — but the config mistake is the same
/// one, so both report through here rather than through two codes for one typo. The permanently
/// failing side matters as much as the permanently passing one: reported as a behavioural failure
/// it would send the author to look at their agent for a defect that is in their config.
fn names_no_tool(tool: &str, variant: &str, why: &'static str) -> Option<Diagnostic> {
    tool.is_empty().then(|| {
        ineffective(
            variant,
            "tool",
            why,
            "Name the tool the assertion is about.",
        )
    })
}

const EMPTY_TOOL_NEVER_FAILS: &str =
    "`tool` is empty, so it names no recorded call and the assertion can never fail.";
const EMPTY_TOOL_NEVER_HOLDS: &str =
    "`tool` is empty, so it names no recorded call and the assertion can never be satisfied.";

/// Reads the `expect` field, which selects the polarity of a policy-mode assertion.
///
/// This was three copies of `expect.as_deref().unwrap_or("pass") == "pass"`. Exact string
/// equality meant every other spelling — `Pass`, `PASS`, `passes`, `true` — silently selected
/// *expect failure* and inverted the assertion, so a config that reads as "this must be allowed"
/// went green precisely when the policy rejected it. An inversion is worse than a no-op: a no-op
/// stops checking, an inversion checks the opposite.
///
/// One function rather than three comparisons, per `AGENTS.md` Verification. The `Err` variant is
/// boxed because `Diagnostic` is large enough to trip `result_large_err` on a `bool` happy path.
fn expected_pass(expect: &Option<String>, variant: &str) -> Result<bool, Box<Diagnostic>> {
    match expect.as_deref() {
        None | Some("pass") => Ok(true),
        Some("fail") => Ok(false),
        Some(other) => Err(Box::new(make_diag(
            "E_CONFIG_ERROR",
            &format!(
                "Assertion `{variant}` has an unrecognized `expect` value; \
                 the only accepted values are `pass` and `fail`."
            ),
            None,
            Some(
                serde_json::json!({ "assertion": variant, "field": "expect", "length": other.len() }),
            ),
        ))),
    }
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
