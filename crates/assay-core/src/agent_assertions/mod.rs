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
            // FALLBACK (PR-406): If no episode found for this run_id,
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
