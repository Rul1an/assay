pub mod matchers;
pub mod model;

use crate::errors::diagnostic::Diagnostic;
use crate::storage::Store;

pub struct EpisodeGraph {
    pub episode_id: String,
    pub steps: Vec<crate::storage::rows::StepRow>,
    pub tool_calls: Vec<crate::storage::rows::ToolCallRow>,
}

pub fn verify_assertions(
    store: &Store,
    run_id: i64,
    test_id: &str,
    assertions: &[model::TraceAssertion],
) -> anyhow::Result<Vec<Diagnostic>> {
    let graph_res = store.get_episode_graph(run_id, test_id);
    match graph_res {
        Ok(graph) => matchers::evaluate(&graph, assertions),
        Err(e) => {
            // FALLBACK 1: Unit Test Mode (Policy Validation)
            // If assertions have explicit `test_args`, `test_trace`, etc., we don't need a real episode.
            // Check if ALL assertions are unit tests.
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "an assertion kind with no test-input field cannot be a unit test; a new kind that carries one must be named above or its unit-test form is not recognised"
            )]
            let is_unit_test = assertions.iter().all(|a| match a {
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
            });

            if is_unit_test {
                // Construct dummy graph
                let dummy = EpisodeGraph {
                    episode_id: "unit_test_mock".into(),
                    steps: vec![],
                    tool_calls: vec![],
                };
                return matchers::evaluate(&dummy, assertions);
            }

            // FALLBACK 2 (PR-406): If no episode found for this run_id,
            // try to find the LATEST episode for this test_id regardless of run_id.
            // This supports the "Demo Flow": Record -> Ingest (Run A) -> Verify (Run B)
            if e.to_string().contains("E_TRACE_EPISODE_MISSING") {
                match store.get_latest_episode_graph_by_test_id(test_id) {
                    Ok(latest_graph) => return matchers::evaluate(&latest_graph, assertions),
                    Err(fallback_err) => {
                        return Err(anyhow::anyhow!("E_TRACE_EPISODE_MISSING: Primary query failed ({}), Fallback failed: {}", e, fallback_err));
                    }
                }
            }

            // Check if error is ambiguous or missing
            // For now, return Err to platform, but ideally convert to Diagnostic
            Err(e)
        }
    }
}
