//! The companion cover for trace assertions (#1949, layer 2, the assertion half).
//!
//! # The case this exists for
//!
//! `matchers::check_one` returns `Option<Diagnostic>`: `Some` is a failure, `None` is a pass. A
//! pass therefore says nothing about whether anything was checked, and for one variant that gap is
//! the whole issue:
//!
//! ```yaml
//! assertions:
//!   - type: trace_must_not_call_tool
//!     tool: delete_repository
//! ```
//!
//! If the agent was never offered `delete_repository`, this holds on every trace forever. It is
//! syntactically perfect, it is *config*-effective — layer 3's `ineffective_reason` passes it
//! correctly, because a trace that called the tool would fail it — and it has never once
//! constrained the agent. #1949's groundwork named it the headline case, and it is the one thing
//! neither the static sweep nor the metric-side cover (#2083) could reach: assertions produce
//! `Diagnostic`s, not `MetricResult`s, so they have no `exercised` dimension to carry.
//!
//! # The antecedent is availability, not absence
//!
//! The naive signal — "the tool was never called" — is the assertion's own passing condition, so
//! reporting it would fire on every `trace_must_not_call_tool` that holds. That is the over-eager
//! detection Beer et al. warn about, and it would be suppressed immediately and deservedly.
//!
//! The signal is whether the tool was ever **available** to the agent:
//!
//! | the agent | verdict |
//! |---|---|
//! | had the tool and did not call it | exercised. A real pass, and the assertion earned it. |
//! | never had the tool | not exercised. No trace could have failed this. |
//! | availability unrecorded | **nothing is reported.** |
//!
//! The third row is what makes this safe to turn on. Availability comes from
//! `meta["tool_definitions"]`, which many traces do not carry; treating "no record" as "no tool"
//! would put a finding on every `trace_must_not_call_tool` in every suite that replays a plain
//! trace. Absence of evidence is not evidence of absence, and the asymmetry is the same one
//! `coverage_regressed` uses for a baseline that predates its dimension (#2082).
//!
//! Tools that were *called* count as available even when no definition list was recorded: a call
//! is proof of availability, and it is proof that does not depend on the producer having written
//! the metadata.
//!
//! # What this deliberately does not claim
//!
//! Availability is a weaker fact than opportunity. An agent may hold a tool that no reachable state
//! would ever prompt it to use, and this reports that as exercised. Trajectory evaluation has the
//! general form of that limit: a single rollout "rules out only one realized path"
//! ([DiagEval, arXiv:2605.17439]), so no per-run signal settles what the agent *could* have done.
//! What this catches is the case that needs no counterfactual at all — the tool was not on the
//! table.
//!
//! [DiagEval, arXiv:2605.17439]: https://arxiv.org/abs/2605.17439

use super::model::TraceAssertion;
use super::EpisodeGraph;

/// The tools an agent could have called during an episode.
///
/// `declared` is `None` when nothing recorded a tool list. That is *unknown*, not *empty*, and
/// every method here keeps the two apart — an empty declared list means "the agent was offered no
/// tools", which is a real and reportable fact.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolAvailability {
    declared: Option<Vec<String>>,
    called: Vec<String>,
}

impl ToolAvailability {
    /// Read what an episode and its response say about which tools existed.
    ///
    /// `meta["tool_definitions"]` is the same field `tool_collision_detect` and
    /// `tool_description_integrity` read, spelled once here so the three cannot disagree about
    /// where a tool list lives.
    pub fn observe(meta: &serde_json::Value, graph: &EpisodeGraph) -> Self {
        let declared = meta
            .get("tool_definitions")
            .and_then(|v| v.as_array())
            .map(|defs| {
                defs.iter()
                    .filter_map(|d| d.get("name").and_then(|n| n.as_str()))
                    .map(str::to_owned)
                    .collect()
            });
        let called = graph
            .tool_calls
            .iter()
            .filter_map(|t| t.tool_name.clone())
            .collect();
        Self { declared, called }
    }

    /// Whether `tool` was available: `None` when nothing recorded enough to say.
    ///
    /// A tri-state on purpose. Collapsing the unknown case into `false` is precisely the mistake
    /// that would make this check noise.
    fn was_available(&self, tool: &str) -> Option<bool> {
        if self.called.iter().any(|c| c == tool) {
            // It was called, so it existed. True regardless of what was declared, and true even
            // when nothing was declared.
            return Some(true);
        }
        let declared = self.declared.as_ref()?;
        Some(declared.iter().any(|d| d == tool))
    }
}

/// One assertion that could not have failed for this run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionCover {
    /// The assertion's `type:` tag, as written in the config.
    pub assertion: String,
    pub reason: String,
}

/// The `type:` tag for an assertion, matching the serde rename in [`TraceAssertion`].
fn tag(a: &TraceAssertion) -> &'static str {
    match a {
        TraceAssertion::TraceMustCallTool { .. } => "trace_must_call_tool",
        TraceAssertion::TraceMustNotCallTool { .. } => "trace_must_not_call_tool",
        TraceAssertion::TraceToolSequence { .. } => "trace_tool_sequence",
        TraceAssertion::TraceMaxSteps { .. } => "trace_max_steps",
        TraceAssertion::ArgsValid { .. } => "args_valid",
        TraceAssertion::SequenceValid { .. } => "sequence_valid",
        TraceAssertion::ToolBlocklist { .. } => "tool_blocklist",
    }
}

/// Why this assertion could not have failed for this run, or `None` if it was genuinely exercised.
///
/// Separate from `check_one` rather than folded into it, and that is not an accident of
/// convenience. "Did this fail" and "was this exercised" are orthogonal questions — the reason
/// `Exercised` is a dimension on `MetricResult` rather than a fourth status. Folding the cover into
/// `check_one` would also disturb `ineffective_reason`, which runs `check_one` against an empty
/// episode and keeps the config-decided codes; a cover that fired on that empty episode would be
/// indistinguishable from a static verdict and the sweep would start rejecting working configs.
///
/// Only two variants can be vacuous at runtime, and the rest are listed here rather than caught by
/// a `_` arm so that a new variant has to be considered:
///
/// - `trace_must_call_tool` compares a count against a minimum on every trace, and a trace with no
///   calls **fails** it. Its antecedent always fires.
/// - `trace_tool_sequence` likewise: an empty actual sequence fails both the subsequence and the
///   exact form.
/// - `args_valid`, `sequence_valid` and `tool_blocklist` evaluate `test_*` fixtures and never read
///   the graph, so a run cannot leave them unexercised. Without those fixtures they check nothing
///   at all, which layer 3 already refuses at config time.
pub fn not_exercised(
    graph: &EpisodeGraph,
    tools: &ToolAvailability,
    a: &TraceAssertion,
) -> Option<AssertionCover> {
    let reason = match a {
        TraceAssertion::TraceMustNotCallTool { tool } => {
            // `Some(false)` only. `None` is "nothing recorded a tool list", which is not evidence
            // that the tool was missing.
            if tools.was_available(tool) == Some(false) {
                Some(format!(
                    "the agent was never offered `{tool}`, so no trace could have called it"
                ))
            } else {
                None
            }
        }
        TraceAssertion::TraceMaxSteps { .. } => {
            // A step ceiling against an episode with no steps compared a budget to nothing. The
            // assertion is fine; the run had nothing to hold it against.
            if graph.steps.is_empty() {
                Some("the episode recorded no steps, so the ceiling was never approached".into())
            } else {
                None
            }
        }
        TraceAssertion::TraceMustCallTool { .. }
        | TraceAssertion::TraceToolSequence { .. }
        | TraceAssertion::ArgsValid { .. }
        | TraceAssertion::SequenceValid { .. }
        | TraceAssertion::ToolBlocklist { .. } => None,
    }?;

    Some(AssertionCover {
        assertion: tag(a).to_string(),
        reason,
    })
}

/// The covers for a whole assertion list.
pub fn evaluate_cover(
    graph: &EpisodeGraph,
    tools: &ToolAvailability,
    assertions: &[TraceAssertion],
) -> Vec<AssertionCover> {
    assertions
        .iter()
        .filter_map(|a| not_exercised(graph, tools, a))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::rows::{StepRow, ToolCallRow};

    fn call(tool: &str) -> ToolCallRow {
        ToolCallRow {
            id: 1,
            step_id: "s1".into(),
            episode_id: "e1".into(),
            tool_name: Some(tool.into()),
            call_index: Some(0),
            args: None,
            result: None,
        }
    }

    fn step() -> StepRow {
        StepRow {
            id: "s1".into(),
            episode_id: "e1".into(),
            idx: 0,
            kind: Some("assistant".into()),
            name: None,
            content: None,
        }
    }

    fn graph(steps: Vec<StepRow>, calls: Vec<ToolCallRow>) -> EpisodeGraph {
        EpisodeGraph {
            episode_id: "e1".into(),
            steps,
            tool_calls: calls,
        }
    }

    fn defs(names: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tool_definitions": names.iter().map(|n| serde_json::json!({"name": n})).collect::<Vec<_>>()
        })
    }

    fn must_not_call(tool: &str) -> TraceAssertion {
        TraceAssertion::TraceMustNotCallTool { tool: tool.into() }
    }

    /// The headline case: the agent was never offered the tool, so the guard never guarded.
    #[test]
    fn a_guard_against_a_tool_the_agent_never_had_is_not_exercised() {
        let g = graph(vec![step()], vec![call("read_file")]);
        let tools = ToolAvailability::observe(&defs(&["read_file", "list_dir"]), &g);
        let cover = not_exercised(&g, &tools, &must_not_call("delete_repository")).unwrap();
        assert_eq!(cover.assertion, "trace_must_not_call_tool");
        assert!(cover.reason.contains("never offered"), "{}", cover.reason);
    }

    /// The pass that was earned: the tool was on the table and the agent left it alone. Reporting
    /// this would fire on every working guard, which is the whole failure mode to avoid.
    #[test]
    fn a_guard_against_an_available_tool_the_agent_declined_is_exercised() {
        let g = graph(vec![step()], vec![call("read_file")]);
        let tools = ToolAvailability::observe(&defs(&["read_file", "delete_repository"]), &g);
        assert_eq!(
            not_exercised(&g, &tools, &must_not_call("delete_repository")),
            None
        );
    }

    /// No tool list recorded is *unknown*, never *absent*. Without this, every suite replaying a
    /// plain trace would get a finding on every guard it has.
    #[test]
    fn an_unrecorded_tool_list_reports_nothing() {
        let g = graph(vec![step()], vec![call("read_file")]);
        let tools = ToolAvailability::observe(&serde_json::json!({}), &g);
        assert_eq!(
            not_exercised(&g, &tools, &must_not_call("delete_repository")),
            None
        );
        assert_eq!(
            tools.was_available("delete_repository"),
            None,
            "unknown, not false"
        );
    }

    /// A tool that was called is available even when nothing declared it: the call is the proof,
    /// and it does not depend on the producer writing the metadata.
    #[test]
    fn a_called_tool_counts_as_available_without_a_declaration() {
        let g = graph(vec![step()], vec![call("delete_repository")]);
        let tools = ToolAvailability::observe(&serde_json::json!({}), &g);
        assert_eq!(tools.was_available("delete_repository"), Some(true));
        assert_eq!(
            not_exercised(&g, &tools, &must_not_call("delete_repository")),
            None
        );
    }

    /// A recorded but empty tool list is a real fact — the agent was offered nothing — and is not
    /// the unknown case.
    #[test]
    fn an_empty_declared_list_is_evidence_and_reports() {
        let g = graph(vec![step()], vec![]);
        let tools = ToolAvailability::observe(&defs(&[]), &g);
        assert_eq!(tools.was_available("anything"), Some(false));
        assert!(not_exercised(&g, &tools, &must_not_call("anything")).is_some());
    }

    #[test]
    fn a_step_ceiling_against_an_empty_episode_is_not_exercised() {
        let g = graph(vec![], vec![]);
        let tools = ToolAvailability::observe(&serde_json::json!({}), &g);
        let cover = not_exercised(&g, &tools, &TraceAssertion::TraceMaxSteps { max: 10 }).unwrap();
        assert!(cover.reason.contains("no steps"), "{}", cover.reason);

        let with_steps = graph(vec![step()], vec![]);
        assert_eq!(
            not_exercised(
                &with_steps,
                &tools,
                &TraceAssertion::TraceMaxSteps { max: 10 }
            ),
            None
        );
    }

    /// `trace_must_call_tool` FAILS on an empty trace, so it is never unexercised. Pinned because
    /// the intuition "no tool calls means nothing was checked" is wrong here, and acting on it
    /// would report a finding alongside a failure that already says more.
    #[test]
    fn a_must_call_assertion_is_never_reported_as_unexercised() {
        let g = graph(vec![], vec![]);
        let tools = ToolAvailability::observe(&serde_json::json!({}), &g);
        assert_eq!(
            not_exercised(
                &g,
                &tools,
                &TraceAssertion::TraceMustCallTool {
                    tool: "read_file".into(),
                    min_calls: Some(1)
                }
            ),
            None
        );
    }

    /// The fixture-driven variants never read the graph, so a run cannot leave them unexercised.
    #[test]
    fn fixture_driven_variants_are_never_reported() {
        let g = graph(vec![], vec![]);
        let tools = ToolAvailability::observe(&serde_json::json!({}), &g);
        let fixtures = [
            TraceAssertion::ArgsValid {
                tool: "t".into(),
                test_args: Some(serde_json::json!({})),
                policy: Some(serde_json::json!({})),
                expect: None,
            },
            TraceAssertion::SequenceValid {
                test_trace: None,
                test_trace_raw: Some(vec![]),
                policy: Some(serde_json::json!({})),
                expect: None,
            },
            TraceAssertion::ToolBlocklist {
                test_tool_calls: Some(vec![]),
                policy: Some(serde_json::json!({})),
                expect: None,
            },
        ];
        for a in &fixtures {
            assert_eq!(not_exercised(&g, &tools, a), None, "{}", tag(a));
        }
    }

    /// The tags match the serde renames, so a config author reads back the word they wrote.
    #[test]
    fn the_tags_match_the_config_vocabulary() {
        let yaml = "type: trace_must_not_call_tool\ntool: x\n";
        let a: TraceAssertion = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(tag(&a), "trace_must_not_call_tool");
    }

    #[test]
    fn evaluate_cover_collects_each_unexercised_assertion() {
        let g = graph(vec![], vec![]);
        let tools = ToolAvailability::observe(&defs(&["read_file"]), &g);
        let covers = evaluate_cover(
            &g,
            &tools,
            &[
                must_not_call("delete_repository"),
                TraceAssertion::TraceMaxSteps { max: 5 },
                TraceAssertion::TraceMustCallTool {
                    tool: "read_file".into(),
                    min_calls: Some(1),
                },
            ],
        );
        assert_eq!(covers.len(), 2);
        assert_eq!(covers[0].assertion, "trace_must_not_call_tool");
        assert_eq!(covers[1].assertion, "trace_max_steps");
    }
}
