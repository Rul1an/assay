pub mod cover;
pub(crate) mod lookup;
pub mod matchers;
pub mod model;

pub(crate) use lookup::EpisodeLookupError;

use crate::errors::diagnostic::Diagnostic;
use crate::storage::Store;

pub struct EpisodeGraph {
    pub episode_id: String,
    pub steps: Vec<crate::storage::rows::StepRow>,
    pub tool_calls: Vec<crate::storage::rows::ToolCallRow>,
}

/// The failures, and the assertions that could not have failed.
///
/// Two lists rather than one: a diagnostic says a check did not hold, a cover says a check was
/// never put to the test. Folding the second into the first would make a coverage observation
/// fail a build, which is the thing #1949's own design forbids.
pub struct AssertionOutcome {
    pub diagnostics: Vec<Diagnostic>,
    pub not_exercised: Vec<cover::AssertionCover>,
}

/// True when the assertion carries its own unit-test input and does not read a stored episode.
pub fn is_unit_test_assertion(a: &model::TraceAssertion) -> bool {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "an assertion kind with no test-input field cannot be a unit test; a new kind that carries one must be named above or its unit-test form is not recognised"
    )]
    match a {
        model::TraceAssertion::ArgsValid { test_args, .. } => test_args.is_some(),
        model::TraceAssertion::SequenceValid {
            test_trace,
            test_trace_raw,
            ..
        } => test_trace.is_some() || test_trace_raw.is_some(),
        model::TraceAssertion::ToolBlocklist {
            test_tool_calls, ..
        } => test_tool_calls.is_some(),
        _ => false,
    }
}

/// True when at least one assertion must read a stored episode.
pub fn assertions_require_stored_episode(assertions: &[model::TraceAssertion]) -> bool {
    !assertions.is_empty() && !assertions.iter().all(is_unit_test_assertion)
}

/// The failures alone, for callers with no response to hand.
///
/// Delegates rather than duplicating: one evaluation, two entry points. `meta` is `Null`, so the
/// companion cover sees no declared tool list and — by the asymmetry in [`cover`] — reports
/// nothing. That is the correct answer for a caller that cannot supply the metadata, and it is
/// why this shortcut is safe to keep for the assertion tests and the published API.
///
/// A production caller that has an `LlmResponse` should use [`verify_assertions_with_meta`]; this
/// one silently has no coverage signal to give.
pub fn verify_assertions(
    store: &Store,
    run_id: i64,
    test_id: &str,
    assertions: &[model::TraceAssertion],
) -> anyhow::Result<Vec<Diagnostic>> {
    verify_assertions_with_meta(store, run_id, test_id, assertions, &serde_json::Value::Null)
        .map(|o| o.diagnostics)
}

pub fn verify_assertions_with_meta(
    store: &Store,
    run_id: i64,
    test_id: &str,
    assertions: &[model::TraceAssertion],
    meta: &serde_json::Value,
) -> anyhow::Result<AssertionOutcome> {
    let finish = |graph: &EpisodeGraph| -> anyhow::Result<AssertionOutcome> {
        let tools = cover::ToolAvailability::observe(meta, graph);
        Ok(AssertionOutcome {
            diagnostics: matchers::evaluate(graph, assertions)?,
            not_exercised: cover::evaluate_cover(graph, &tools, assertions),
        })
    };

    let graph_res = store.get_episode_graph(run_id, test_id);
    match graph_res {
        Ok(graph) => finish(&graph),
        Err(e) => {
            // FALLBACK 1: Unit Test Mode (Policy Validation)
            // If assertions have explicit `test_args`, `test_trace`, etc., we don't need a real episode.
            if assertions.iter().all(is_unit_test_assertion) {
                // Construct dummy graph
                let dummy = EpisodeGraph {
                    episode_id: "unit_test_mock".into(),
                    steps: vec![],
                    tool_calls: vec![],
                };
                return finish(&dummy);
            }

            // Latest-per-test_id only for a typed miss, and only when this
            // invocation opted in. Ambiguous input and database failures stay
            // what they are.
            if e.downcast_ref::<EpisodeLookupError>()
                .is_some_and(EpisodeLookupError::is_missing)
            {
                if store.latest_stored_episode_eval()? {
                    match store.get_latest_episode_graph_by_test_id(test_id) {
                        Ok(latest_graph) => {
                            store.mark_latest_stored_episode_used(test_id)?;
                            return finish(&latest_graph);
                        }
                        Err(fallback_err) => {
                            if fallback_err
                                .downcast_ref::<EpisodeLookupError>()
                                .is_some_and(EpisodeLookupError::is_missing)
                            {
                                return Err(e);
                            }
                            return Err(fallback_err);
                        }
                    }
                }
                return Err(e);
            }

            Err(e)
        }
    }
}
